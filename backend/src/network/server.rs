// WebSocket gateway. Binds a TCP listener on the given address and spawns one
// tokio task per accepted connection. Each task performs the WS upgrade and
// runs the per-connection loop in `handler`.

use std::net::SocketAddr;

use tokio::net::TcpListener;

use super::handler;
use crate::states::app_state::AppState;

pub async fn run(addr: SocketAddr, state: AppState) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    eprintln!("server listening on ws://{addr}");

    loop {
        let (socket, peer) = match listener.accept().await {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("accept error: {e}");
                continue;
            }
        };
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = handler::handle_connection(socket, peer, state).await {
                eprintln!("connection {peer} closed with error: {e}");
            }
        });
    }
}
