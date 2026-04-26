// Playlist controller. Owns the locking strategy: the pure playlist_service
// receives read-only snapshots and returns new values; this layer commits
// them by inserting into the playlists RwLock.

use serde_json::json;

use crate::domain::playlist::PlaylistId;
use crate::domain::song::SongId;
use crate::error::AppError;
use crate::network::protocol::{
    Criteria, ErrorCode, PlaylistDetailDto, PlaylistDto, PlaylistSummaryDto, Response, SongDto,
    SortBy, SortOrder, TransformOp,
};
use crate::services::playlist_service;
use crate::states::app_state::AppState;

pub fn create(state: &AppState, req_id: String, name: String, owner: String) -> Response {
    let resp = {
        let mut guard = match state.playlists.write() {
            Ok(g) => g,
            Err(_) => return server_err(req_id, "CREATE_PLAYLIST", "playlists lock poisoned"),
        };
        match playlist_service::create(&guard, &name, &owner) {
            Ok(new) => {
                let dto = PlaylistDto::from(&new);
                guard.insert(new.id.clone(), new);
                Response::ok(req_id, "CREATE_PLAYLIST", json!({ "playlist": dto }))
            }
            Err(e) => return err(req_id, "CREATE_PLAYLIST", e),
        }
    };
    state.persist_playlists();
    resp
}

pub fn delete(state: &AppState, req_id: String, pid: PlaylistId) -> Response {
    let removed = {
        let mut guard = match state.playlists.write() {
            Ok(g) => g,
            Err(_) => return server_err(req_id, "DELETE_PLAYLIST", "playlists lock poisoned"),
        };
        guard.remove(&pid)
    };
    match removed {
        Some(_) => {
            state.persist_playlists();
            Response::ok(
                req_id,
                "DELETE_PLAYLIST",
                json!({ "playlist_id": pid, "deleted": true }),
            )
        }
        None => Response::err(
            req_id,
            "DELETE_PLAYLIST",
            ErrorCode::NotFound,
            format!("Playlist {pid} not found"),
        ),
    }
}

pub fn list(state: &AppState, req_id: String, owner: Option<String>) -> Response {
    let guard = match state.playlists.read() {
        Ok(g) => g,
        Err(_) => return server_err(req_id, "LIST_PLAYLISTS", "playlists lock poisoned"),
    };
    let dtos: Vec<PlaylistSummaryDto> = guard
        .values()
        .filter(|p| owner.as_deref().map_or(true, |o| p.owner == o))
        .map(PlaylistSummaryDto::from)
        .collect();
    Response::ok(req_id, "LIST_PLAYLISTS", json!({ "playlists": dtos }))
}

pub fn get(state: &AppState, req_id: String, pid: PlaylistId) -> Response {
    let p = {
        let guard = match state.playlists.read() {
            Ok(g) => g,
            Err(_) => return server_err(req_id, "GET_PLAYLIST", "playlists lock poisoned"),
        };
        match guard.get(&pid) {
            Some(p) => p.clone(),
            None => {
                return Response::err(
                    req_id,
                    "GET_PLAYLIST",
                    ErrorCode::NotFound,
                    format!("Playlist {pid} not found"),
                );
            }
        }
    };
    let lib_guard = match state.library.read() {
        Ok(g) => g,
        Err(_) => return server_err(req_id, "GET_PLAYLIST", "library lock poisoned"),
    };
    let songs = playlist_service::materialize(&lib_guard, &p);
    let dto = PlaylistDetailDto {
        id: p.id.clone(),
        name: p.name.clone(),
        owner: p.owner.clone(),
        song_ids: p.song_ids.iter().cloned().collect(),
        songs: songs.iter().map(SongDto::from).collect(),
        created_at: p.created_at,
        version: p.version,
    };
    Response::ok(req_id, "GET_PLAYLIST", json!({ "playlist": dto }))
}

pub fn add_song_to_playlist(
    state: &AppState,
    req_id: String,
    pid: PlaylistId,
    sid: SongId,
) -> Response {
    let lib_snap = state.snapshot_library();
    let resp = {
        let mut guard = match state.playlists.write() {
            Ok(g) => g,
            Err(_) => return server_err(req_id, "ADD_SONG_TO_PLAYLIST", "playlists lock poisoned"),
        };
        match playlist_service::add_song(&lib_snap, &guard, &pid, &sid) {
            Ok(new) => {
                let dto = PlaylistDto::from(&new);
                guard.insert(pid, new);
                Response::ok(req_id, "ADD_SONG_TO_PLAYLIST", json!({ "playlist": dto }))
            }
            Err(e) => return err(req_id, "ADD_SONG_TO_PLAYLIST", e),
        }
    };
    state.persist_playlists();
    resp
}

pub fn remove_song_from_playlist(
    state: &AppState,
    req_id: String,
    pid: PlaylistId,
    sid: SongId,
) -> Response {
    let resp = {
        let mut guard = match state.playlists.write() {
            Ok(g) => g,
            Err(_) => {
                return server_err(req_id, "REMOVE_SONG_FROM_PLAYLIST", "playlists lock poisoned");
            }
        };
        match playlist_service::remove_song(&guard, &pid, &sid) {
            Ok(new) => {
                let dto = PlaylistDto::from(&new);
                guard.insert(pid, new);
                Response::ok(req_id, "REMOVE_SONG_FROM_PLAYLIST", json!({ "playlist": dto }))
            }
            Err(e) => return err(req_id, "REMOVE_SONG_FROM_PLAYLIST", e),
        }
    };
    state.persist_playlists();
    resp
}

pub fn filter(state: &AppState, req_id: String, pid: PlaylistId, c: Criteria) -> Response {
    let lib_snap = state.snapshot_library();
    let pl_guard = match state.playlists.read() {
        Ok(g) => g,
        Err(_) => return server_err(req_id, "FILTER_PLAYLIST", "playlists lock poisoned"),
    };
    match playlist_service::filter(&lib_snap, &pl_guard, &pid, &c) {
        Ok(songs) => {
            let dtos: Vec<SongDto> = songs.iter().map(SongDto::from).collect();
            let count = dtos.len();
            Response::ok(
                req_id,
                "FILTER_PLAYLIST",
                json!({
                    "source_playlist_id": pid,
                    "filtered_songs": dtos,
                    "count": count,
                }),
            )
        }
        Err(e) => err(req_id, "FILTER_PLAYLIST", e),
    }
}

pub fn sort(
    state: &AppState,
    req_id: String,
    pid: PlaylistId,
    by: SortBy,
    order: SortOrder,
) -> Response {
    let lib_snap = state.snapshot_library();
    let pl_guard = match state.playlists.read() {
        Ok(g) => g,
        Err(_) => return server_err(req_id, "SORT_PLAYLIST", "playlists lock poisoned"),
    };
    match playlist_service::sort(&lib_snap, &pl_guard, &pid, by, order) {
        Ok(songs) => {
            let dtos: Vec<SongDto> = songs.iter().map(SongDto::from).collect();
            Response::ok(
                req_id,
                "SORT_PLAYLIST",
                json!({
                    "source_playlist_id": pid,
                    "sorted_songs": dtos,
                }),
            )
        }
        Err(e) => err(req_id, "SORT_PLAYLIST", e),
    }
}

pub fn transform(
    state: &AppState,
    req_id: String,
    pid: PlaylistId,
    op: TransformOp,
    n: Option<usize>,
) -> Response {
    let lib_snap = state.snapshot_library();
    let pl_guard = match state.playlists.read() {
        Ok(g) => g,
        Err(_) => return server_err(req_id, "TRANSFORM_PLAYLIST", "playlists lock poisoned"),
    };
    match playlist_service::transform(&lib_snap, &pl_guard, &pid, op, n) {
        Ok(songs) => {
            let dtos: Vec<SongDto> = songs.iter().map(SongDto::from).collect();
            Response::ok(
                req_id,
                "TRANSFORM_PLAYLIST",
                json!({
                    "source_playlist_id": pid,
                    "result_songs": dtos,
                }),
            )
        }
        Err(e) => err(req_id, "TRANSFORM_PLAYLIST", e),
    }
}

fn err(req_id: String, resp_type: &str, e: AppError) -> Response {
    Response::err(req_id, resp_type, e.code, e.message)
}

fn server_err(req_id: String, resp_type: &str, msg: &str) -> Response {
    Response::err(req_id, resp_type, ErrorCode::ServerError, msg)
}
