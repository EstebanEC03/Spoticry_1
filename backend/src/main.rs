mod cli;
mod controllers;
mod domain;
mod error;
mod network;
mod repositories;
mod services;
mod states;

use std::net::SocketAddr;
use std::path::PathBuf;

use crate::repositories::file_repository;
use crate::states::app_state::AppState;

const BIND_ADDR: &str = "0.0.0.0:7878";
const LIBRARY_PATH: &str = "./data/library.json";
const PLAYLISTS_PATH: &str = "./data/playlists.json";

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let library_path = PathBuf::from(LIBRARY_PATH);
    let playlists_path = PathBuf::from(PLAYLISTS_PATH);

    let library = file_repository::load_library(&library_path)?;
    let playlists = file_repository::load_playlists(&playlists_path)?;
    println!(
        "loaded {} songs, {} playlists from disk",
        library.len(),
        playlists.len()
    );

    let state = AppState::with_data(library, playlists, library_path, playlists_path);
    let server_state = state.clone();
    let cli_state = state.clone();

    let addr: SocketAddr = BIND_ADDR.parse().expect("invalid bind address");
    println!("SpotiCry backend starting on ws://{addr}");

    let server = tokio::spawn(async move {
        if let Err(e) = network::server::run(addr, server_state).await {
            eprintln!("server error: {e}");
        }
    });

    let cli_task = tokio::spawn(async move {
        if let Err(e) = cli::commands::run(cli_state).await {
            eprintln!("cli error: {e}");
        }
    });

    // Save on Ctrl+C / SIGINT too — RAM-only mutations between snapshots
    // would otherwise vanish on hard shutdown.
    let shutdown = tokio::signal::ctrl_c();

    tokio::select! {
        r = server => eprintln!("server task ended: {r:?}"),
        r = cli_task => eprintln!("cli task ended: {r:?}"),
        _ = shutdown => eprintln!("ctrl-c received"),
    }

    println!("saving final snapshot");
    state.persist_library();
    state.persist_playlists();
    Ok(())
}
