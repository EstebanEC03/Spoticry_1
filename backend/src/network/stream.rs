// Audio streaming pump. Reads the source MP3 in 16 KiB chunks, base64-encodes
// each, and writes a STREAM_CHUNK frame to the shared WS sink. Cooperates with
// STOP_SONG via a oneshot::Receiver — the controller hands us the rx, and the
// matching tx lives inside the StreamHandle stored in AppState; dropping that
// handle (STOP_SONG, connection close, REMOVE_SONG via state cleanup) signals
// us to bail out cleanly.

use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use futures_util::{SinkExt, stream::SplitSink};
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::sync::{Mutex, oneshot};
use tokio_tungstenite::{WebSocketStream, tungstenite::Message};

use crate::network::protocol::StreamChunk;

pub type WsSink = SplitSink<WebSocketStream<TcpStream>, Message>;
pub type SharedSink = Arc<Mutex<WsSink>>;

const CHUNK_SIZE: usize = 16 * 1024;

pub async fn pump(
    path: PathBuf,
    stream_id: String,
    sink: SharedSink,
    cancel_rx: oneshot::Receiver<()>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut file = tokio::fs::File::open(&path).await?;
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut seq: u64 = 0;
    let mut cancel = cancel_rx;

    loop {
        tokio::select! {
            biased;
            _ = &mut cancel => {
                // Sender dropped (STOP_SONG / connection close / state purge)
                // or explicit cancel — bail without sending more chunks.
                return Ok(());
            }
            result = file.read(&mut buf) => {
                let n = result?;
                if n == 0 {
                    let chunk = StreamChunk::new(stream_id.clone(), seq, String::new(), true);
                    send_chunk(&sink, &chunk).await?;
                    return Ok(());
                }
                let payload_b64 = B64.encode(&buf[..n]);
                let chunk = StreamChunk::new(stream_id.clone(), seq, payload_b64, false);
                send_chunk(&sink, &chunk).await?;
                seq = seq.saturating_add(1);
            }
        }
    }
}

async fn send_chunk(sink: &SharedSink, chunk: &StreamChunk) -> Result<(), Box<dyn Error + Send + Sync>> {
    let json = serde_json::to_string(chunk)?;
    let mut guard = sink.lock().await;
    guard.send(Message::Text(json.into())).await?;
    Ok(())
}
