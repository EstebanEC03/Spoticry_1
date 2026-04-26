// Server-side CLI loop. Reads stdin lines, dispatches to song / playlist
// services. Each successful mutation persists the relevant snapshot so a
// crash between mutations cannot lose data.

use std::path::PathBuf;

use tokio::io::{AsyncBufReadExt, BufReader};

use crate::domain::playlist::Playlist;
use crate::services::playlist_service;
use crate::services::song_service::{self, SongOverrides};
use crate::states::app_state::AppState;

pub async fn run(state: AppState) -> std::io::Result<()> {
    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();
    println!(
        "CLI ready. Commands: add | remove | list | playlists | create-playlist | delete-playlist | help | quit"
    );

    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, char::is_whitespace);
        let cmd = parts.next().unwrap_or("");
        let arg = parts.next().unwrap_or("").trim();

        match cmd {
            "add" => {
                handle_add(&state, arg);
                state.persist_library();
            }
            "remove" => {
                handle_remove(&state, arg);
                state.persist_library();
            }
            "list" => handle_list(&state),
            "playlists" => handle_playlists(&state),
            "create-playlist" => {
                if handle_create_playlist(&state, arg) {
                    state.persist_playlists();
                }
            }
            "delete-playlist" => {
                if handle_delete_playlist(&state, arg) {
                    state.persist_playlists();
                }
            }
            "quit" | "exit" => {
                println!("CLI shutting down — saving snapshots");
                state.persist_library();
                state.persist_playlists();
                break;
            }
            "help" | "?" => print_help(),
            other => println!("ERROR  unknown command: {other}"),
        }
    }
    Ok(())
}

fn handle_add(state: &AppState, arg: &str) {
    if arg.is_empty() {
        println!("ERROR  usage: add <path> [| <title> | <artist> | <genre>]");
        return;
    }
    // Pipe-separated optional overrides: path | title | artist | genre.
    // Any empty field falls back to ID3 → filename stem → "Unknown".
    let mut parts = arg.splitn(4, '|').map(str::trim);
    let raw_path = parts.next().unwrap_or("");
    if raw_path.is_empty() {
        println!("ERROR  usage: add <path> [| <title> | <artist> | <genre>]");
        return;
    }
    let overrides = SongOverrides {
        title: parts.next().filter(|s| !s.is_empty()).map(String::from),
        artist: parts.next().filter(|s| !s.is_empty()).map(String::from),
        genre: parts.next().filter(|s| !s.is_empty()).map(String::from),
    };
    let path = PathBuf::from(raw_path);
    match song_service::add_from_path(state, &path, overrides) {
        Ok(song) => println!(
            "OK  song_id={}  title=\"{}\"  artist=\"{}\"  genre=\"{}\"  duration={}s  path={}",
            song.id,
            song.title,
            song.artist,
            song.genre,
            song.duration_sec,
            song.path.display()
        ),
        Err(e) => println!("ERROR  {:?}  {}", e.code, e.message),
    }
}

fn handle_remove(state: &AppState, arg: &str) {
    if arg.is_empty() {
        println!("ERROR  usage: remove <song_id>");
        return;
    }
    match song_service::remove_song(state, &arg.to_string()) {
        Ok(song) => println!("OK  song_id={}  removed=true  title=\"{}\"", song.id, song.title),
        Err(e) => println!("ERROR  {:?}  {}", e.code, e.message),
    }
}

fn handle_list(state: &AppState) {
    match song_service::list_songs(state, None) {
        Ok(songs) => {
            println!("OK  count={}", songs.len());
            for s in &songs {
                println!(
                    "  {}  \"{}\"  by {}  ({}s, {}bps)  {}",
                    s.id,
                    s.title,
                    s.artist,
                    s.duration_sec,
                    s.bitrate,
                    s.path.display()
                );
            }
        }
        Err(e) => println!("ERROR  {:?}  {}", e.code, e.message),
    }
}

fn handle_playlists(state: &AppState) {
    let snap = state.snapshot_playlists();
    println!("OK  count={}", snap.len());
    for p in snap.values() {
        println!(
            "  {}  \"{}\"  owner={}  songs={}  v{}",
            p.id,
            p.name,
            p.owner,
            p.song_count(),
            p.version
        );
    }
}

fn handle_create_playlist(state: &AppState, arg: &str) -> bool {
    // Pipe-separated: <name> [| <owner>]. Owner defaults to "cli".
    if arg.is_empty() {
        println!("ERROR  usage: create-playlist <name> [| <owner>]");
        return false;
    }
    let mut parts = arg.splitn(2, '|').map(str::trim);
    let name = parts.next().unwrap_or("");
    let owner = parts.next().filter(|s| !s.is_empty()).unwrap_or("cli");
    if name.is_empty() {
        println!("ERROR  usage: create-playlist <name> [| <owner>]");
        return false;
    }

    let mut guard = match state.playlists.write() {
        Ok(g) => g,
        Err(_) => {
            println!("ERROR  ServerError  playlists lock poisoned");
            return false;
        }
    };
    match playlist_service::create(&guard, name, owner) {
        Ok(new) => {
            let summary = describe(&new);
            guard.insert(new.id.clone(), new);
            println!("OK  {summary}");
            true
        }
        Err(e) => {
            println!("ERROR  {:?}  {}", e.code, e.message);
            false
        }
    }
}

fn handle_delete_playlist(state: &AppState, arg: &str) -> bool {
    if arg.is_empty() {
        println!("ERROR  usage: delete-playlist <playlist_id>");
        return false;
    }
    let mut guard = match state.playlists.write() {
        Ok(g) => g,
        Err(_) => {
            println!("ERROR  ServerError  playlists lock poisoned");
            return false;
        }
    };
    match guard.remove(arg) {
        Some(p) => {
            println!("OK  playlist_id={}  name=\"{}\"  deleted=true", p.id, p.name);
            true
        }
        None => {
            println!("ERROR  NotFound  playlist {arg} not found");
            false
        }
    }
}

fn describe(p: &Playlist) -> String {
    format!(
        "playlist_id={}  name=\"{}\"  owner={}  songs={}  v{}",
        p.id,
        p.name,
        p.owner,
        p.song_count(),
        p.version
    )
}

fn print_help() {
    println!("Commands:");
    println!("  add <path> [| <title> | <artist> | <genre>]");
    println!("                      add an MP3. Empty fields fall back to ID3 tags,");
    println!("                      then filename stem (title) / \"Unknown\".");
    println!("                      Example: add foo.mp3 || Some Artist | Rock");
    println!("  remove <song_id>    remove a song (fails if currently streaming)");
    println!("  list                list all songs");
    println!("  playlists           list all playlists");
    println!("  create-playlist <name> [| <owner>]");
    println!("                      create a playlist. Owner defaults to \"cli\".");
    println!("  delete-playlist <playlist_id>");
    println!("                      delete a playlist by id");
    println!("  quit                save snapshots and exit the CLI loop");
}
