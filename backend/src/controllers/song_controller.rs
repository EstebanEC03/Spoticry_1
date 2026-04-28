// Song controller — translates RequestPayload variants into Response envelopes.
// PLAY_SONG returns a richer PlayResult so the handler can spawn a stream
// task and own the WebSocket sink; the controller only sets up the metadata
// ack and reserves the stream slot in AppState.

use std::path::PathBuf;

use serde_json::json;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::domain::song::SongId;
use crate::error::AppError;
use crate::network::protocol::{Criteria, ErrorCode, Response, SongDto};
use crate::services::song_service;
use crate::states::app_state::AppState;

const CHUNK_SIZE: usize = 16 * 1024;
const MIME_MP3: &str = "audio/mpeg";

pub enum PlayResult {
    Ack {
        response: Response,
        stream_id: String,
        path: PathBuf,
        cancel_rx: oneshot::Receiver<()>,
    },
    Err(Response),
}

pub fn list_songs(state: &AppState, req_id: String, criteria: Option<&Criteria>) -> Response {
    match song_service::list_songs(state, criteria) {
        Ok(songs) => {
            let dtos: Vec<SongDto> = songs.iter().map(SongDto::from).collect();
            Response::ok(
                req_id,
                "LIST_SONGS",
                json!({ "count": dtos.len(), "songs": dtos }),
            )
        }
        Err(e) => err(req_id, "LIST_SONGS", e),
    }
}

pub fn get_song(state: &AppState, req_id: String, song_id: SongId) -> Response {
    match song_service::get_song(state, &song_id) {
        Ok(song) => Response::ok(
            req_id,
            "GET_SONG",
            json!({ "song": SongDto::from(&song) }),
        ),
        Err(e) => err(req_id, "GET_SONG", e),
    }
}

pub fn play_song(state: &AppState, req_id: String, song_id: SongId) -> PlayResult {
    let song = match song_service::get_song(state, &song_id) {
        Ok(s) => s,
        Err(e) => return PlayResult::Err(err(req_id, "PLAY_SONG", e)),
    };
    let resolved_path = state.resolve_song_path(&song.path);
    let total_bytes = match std::fs::metadata(&resolved_path) {
        Ok(m) => m.len(),
        Err(e) => {
            return PlayResult::Err(Response::err(
                req_id,
                "PLAY_SONG",
                ErrorCode::ServerError,
                format!("Cannot read song file {}: {e}", resolved_path.display()),
            ));
        }
    };
    let stream_id = format!("str-{}", &Uuid::new_v4().simple().to_string()[..8]);
    let (cancel_tx, cancel_rx) = oneshot::channel();
    if !state.register_stream(stream_id.clone(), song_id.clone(), cancel_tx) {
        return PlayResult::Err(Response::err(
            req_id,
            "PLAY_SONG",
            ErrorCode::ServerError,
            "streams lock poisoned",
        ));
    }
    let response = Response::ok(
        req_id,
        "PLAY_SONG",
        json!({
            "stream_id": stream_id,
            "song_id": song_id,
            "mime": MIME_MP3,
            "total_bytes": total_bytes,
            "chunk_size": CHUNK_SIZE,
            "duration_sec": song.duration_sec,
        }),
    );
    PlayResult::Ack {
        response,
        stream_id,
        path: resolved_path,
        cancel_rx,
    }
}

pub fn stop_song(state: &AppState, req_id: String, stream_id: String) -> Response {
    match state.unregister_stream(&stream_id) {
        Some(_) => Response::ok(
            req_id,
            "STOP_SONG",
            json!({ "stream_id": stream_id, "stopped": true }),
        ),
        None => Response::err(
            req_id,
            "STOP_SONG",
            ErrorCode::StreamNotFound,
            format!("Stream {stream_id} not active"),
        ),
    }
}

fn err(req_id: String, resp_type: &str, e: AppError) -> Response {
    Response::err(req_id, resp_type, e.code, e.message)
}
