// Per-connection WebSocket handler. Upgrades the raw TCP stream to WS, then
// loops: read text frame → deserialize RequestEnvelope → dispatch → write text
// frame with JSON Response. PLAY_SONG / STOP_SONG receive special handling:
// the sink is shared with the spawned stream pump, and per-connection state
// enforces ALREADY_PLAYING.

use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;

use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::net::TcpStream;
use tokio::sync::Mutex as AsyncMutex;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use uuid::Uuid;

use crate::controllers::{playlist_controller, song_controller};
use crate::controllers::song_controller::PlayResult;
use crate::network::protocol::{ErrorCode, RequestEnvelope, RequestPayload, Response};
use crate::network::stream::{self, SharedSink};
use crate::states::app_state::AppState;

const SERVER_VERSION: &str = "1.0";

pub async fn handle_connection(
    socket: TcpStream,
    peer: SocketAddr,
    state: AppState,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let ws = accept_async(socket).await?;
    let (write, mut read) = ws.split();
    let sink: SharedSink = Arc::new(AsyncMutex::new(write));
    let mut current_stream: Option<String> = None;

    eprintln!("ws connection opened from {peer}");

    // Forward server-side broadcast notifications (LIBRARY_UPDATED, …) to this
    // connection over the same WS channel. Aborted on connection close so we
    // don't leak tasks per dropped client.
    let notif_task = {
        let mut rx = state.notifications.subscribe();
        let notif_sink = sink.clone();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(notif) => {
                        let json = match serde_json::to_string(&notif) {
                            Ok(s) => s,
                            Err(_) => continue,
                        };
                        let mut guard = notif_sink.lock().await;
                        if guard.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        })
    };

    while let Some(msg) = read.next().await {
        let msg = msg?;
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Binary(b) => match std::str::from_utf8(&b) {
                Ok(s) => s.to_string(),
                Err(_) => {
                    send_json(
                        &sink,
                        &Response::err(
                            String::new(),
                            "ERROR",
                            ErrorCode::InvalidRequest,
                            "Binary frames must be UTF-8 JSON",
                        ),
                    )
                    .await?;
                    continue;
                }
            },
            Message::Ping(p) => {
                let mut guard = sink.lock().await;
                guard.send(Message::Pong(p)).await?;
                continue;
            }
            Message::Close(_) => break,
            _ => continue,
        };

        let env: RequestEnvelope = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                let resp = Response::err(
                    String::new(),
                    "ERROR",
                    ErrorCode::InvalidRequest,
                    format!("Malformed JSON or missing type: {e}"),
                );
                send_json(&sink, &resp).await?;
                continue;
            }
        };
        let id = env.id.unwrap_or_default();

        match env.payload {
            RequestPayload::PlaySong { song_id } => {
                if current_stream.is_some() {
                    let resp = Response::err(
                        id,
                        "PLAY_SONG",
                        ErrorCode::AlreadyPlaying,
                        "Client already has an active stream",
                    );
                    send_json(&sink, &resp).await?;
                    continue;
                }
                match song_controller::play_song(&state, id, song_id) {
                    PlayResult::Ack {
                        response,
                        stream_id,
                        path,
                        cancel_rx,
                    } => {
                        send_json(&sink, &response).await?;
                        current_stream = Some(stream_id.clone());
                        let task_state = state.clone();
                        let task_sink = sink.clone();
                        let task_stream_id = stream_id.clone();
                        tokio::spawn(async move {
                            // Pump finishes (EOF) far before client finishes
                            // playback — server sends 16 KiB chunks back-to-back
                            // at network speed. Spec §6 forbids removing a
                            // song "siendo reproducida" (still being played),
                            // not "still being pumped". So on clean EOF we
                            // KEEP the stream registered: it clears only on
                            // explicit STOP_SONG or on connection close. On
                            // pump error (file gone, sink dead) we drop it
                            // since the client cannot still be playing.
                            match stream::pump(
                                path,
                                task_stream_id.clone(),
                                task_sink,
                                cancel_rx,
                            )
                            .await
                            {
                                Ok(()) => {}
                                Err(e) => {
                                    eprintln!("stream {task_stream_id} pump error: {e}");
                                    task_state.unregister_stream(&task_stream_id);
                                }
                            }
                        });
                    }
                    PlayResult::Err(resp) => send_json(&sink, &resp).await?,
                }
            }
            RequestPayload::StopSong { stream_id } => {
                let matches_current = current_stream.as_deref() == Some(stream_id.as_str());
                let resp = song_controller::stop_song(&state, id, stream_id);
                send_json(&sink, &resp).await?;
                if matches_current {
                    current_stream = None;
                }
            }
            other => {
                let resp = dispatch(&state, id, other);
                send_json(&sink, &resp).await?;
            }
        }
    }

    // Connection closing — cancel any active stream this client owned and
    // tear down the notification forwarder.
    if let Some(sid) = current_stream.take() {
        state.unregister_stream(&sid);
    }
    notif_task.abort();
    eprintln!("ws connection closed from {peer}");
    Ok(())
}

fn dispatch(state: &AppState, id: String, payload: RequestPayload) -> Response {
    match payload {
        RequestPayload::Hello { client_id, version } => {
            eprintln!("hello from client_id={client_id} version={version}");
            if version != SERVER_VERSION {
                return Response::err(
                    id,
                    "HELLO",
                    ErrorCode::VersionMismatch,
                    format!("Server speaks {SERVER_VERSION}, client sent {version}"),
                );
            }
            let session = format!("sess-{}", &Uuid::new_v4().simple().to_string()[..8]);
            Response::ok(
                id,
                "HELLO",
                json!({
                    "server_version": SERVER_VERSION,
                    "session_id": session,
                    "supported_ops": SUPPORTED_OPS,
                }),
            )
        }
        RequestPayload::Ping { ts } => {
            let server_ts = Utc::now().timestamp_millis();
            Response::ok(
                id,
                "PONG",
                json!({
                    "ts": ts.unwrap_or(0),
                    "server_ts": server_ts,
                }),
            )
        }
        RequestPayload::ListSongs { criteria } => {
            song_controller::list_songs(state, id, criteria.as_ref())
        }
        RequestPayload::GetSong { song_id } => song_controller::get_song(state, id, song_id),
        RequestPayload::CreatePlaylist { name, owner } => {
            playlist_controller::create(state, id, name, owner)
        }
        RequestPayload::DeletePlaylist { playlist_id } => {
            playlist_controller::delete(state, id, playlist_id)
        }
        RequestPayload::ListPlaylists { owner } => playlist_controller::list(state, id, owner),
        RequestPayload::GetPlaylist { playlist_id } => {
            playlist_controller::get(state, id, playlist_id)
        }
        RequestPayload::AddSongToPlaylist {
            playlist_id,
            song_id,
        } => playlist_controller::add_song_to_playlist(state, id, playlist_id, song_id),
        RequestPayload::RemoveSongFromPlaylist {
            playlist_id,
            song_id,
        } => playlist_controller::remove_song_from_playlist(state, id, playlist_id, song_id),
        RequestPayload::FilterPlaylist {
            playlist_id,
            criteria,
        } => playlist_controller::filter(state, id, playlist_id, criteria),
        RequestPayload::SortPlaylist {
            playlist_id,
            by,
            order,
        } => playlist_controller::sort(state, id, playlist_id, by, order),
        RequestPayload::TransformPlaylist {
            playlist_id,
            op,
            n,
        } => playlist_controller::transform(state, id, playlist_id, op, n),
        // PLAY_SONG and STOP_SONG handled inline in handle_connection.
        RequestPayload::PlaySong { .. } | RequestPayload::StopSong { .. } => unreachable!(),
    }
}

const SUPPORTED_OPS: &[&str] = &[
    "HELLO",
    "PING",
    "LIST_SONGS",
    "GET_SONG",
    "PLAY_SONG",
    "STOP_SONG",
    "CREATE_PLAYLIST",
    "DELETE_PLAYLIST",
    "LIST_PLAYLISTS",
    "GET_PLAYLIST",
    "ADD_SONG_TO_PLAYLIST",
    "REMOVE_SONG_FROM_PLAYLIST",
    "FILTER_PLAYLIST",
    "SORT_PLAYLIST",
    "TRANSFORM_PLAYLIST",
];

async fn send_json(sink: &SharedSink, resp: &Response) -> Result<(), Box<dyn Error + Send + Sync>> {
    let json = serde_json::to_string(resp)?;
    let mut guard = sink.lock().await;
    guard.send(Message::Text(json.into())).await?;
    Ok(())
}
