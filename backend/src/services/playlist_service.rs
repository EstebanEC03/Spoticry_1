// Pure service layer for playlists. No locking, no I/O, no interior mutation.
// Operates on read-only snapshots of library and playlists; returns brand new
// values for the caller (controller) to commit. Playlist transformations are
// delegated to the pure functions in `crate::domain::playlist`.

use std::collections::HashMap;

use crate::domain::playlist::{self, Playlist, PlaylistError, PlaylistId};
use crate::domain::song::{Song, SongId};
use crate::error::AppError;
use crate::network::protocol::{
    Criteria, CriteriaField, CriteriaOp, ErrorCode, SortBy, SortOrder, TransformOp,
};

pub type Library = HashMap<SongId, Song>;
pub type Playlists = HashMap<PlaylistId, Playlist>;

// --- Commands -------------------------------------------------------------

pub fn create(existing: &Playlists, name: &str, owner: &str) -> Result<Playlist, AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::new(
            ErrorCode::InvalidRequest,
            "Playlist name cannot be empty",
        ));
    }
    let dup = existing
        .values()
        .any(|p| p.name == trimmed && p.owner == owner);
    if dup {
        return Err(AppError::new(
            ErrorCode::DuplicateName,
            "Playlist name already exists",
        ));
    }
    Ok(Playlist::new(trimmed.to_string(), owner.to_string()))
}

pub fn add_song(
    lib: &Library,
    playlists: &Playlists,
    pid: &PlaylistId,
    sid: &SongId,
) -> Result<Playlist, AppError> {
    if !lib.contains_key(sid) {
        return Err(AppError::new(
            ErrorCode::SongNotFound,
            format!("Song {sid} not found"),
        ));
    }
    let p = lookup(playlists, pid)?;
    playlist::add_song(p, sid.clone()).map_err(map_playlist_err)
}

pub fn remove_song(
    playlists: &Playlists,
    pid: &PlaylistId,
    sid: &SongId,
) -> Result<Playlist, AppError> {
    let p = lookup(playlists, pid)?;
    playlist::remove_song(p, sid).map_err(map_playlist_err)
}

// --- Queries --------------------------------------------------------------

pub fn filter(
    lib: &Library,
    playlists: &Playlists,
    pid: &PlaylistId,
    c: &Criteria,
) -> Result<Vec<Song>, AppError> {
    let p = lookup(playlists, pid)?;
    let pred = criteria_predicate(c)?;
    Ok(playlist::filter_by(p, lib, |s| pred(s)))
}

pub fn sort(
    lib: &Library,
    playlists: &Playlists,
    pid: &PlaylistId,
    by: SortBy,
    order: SortOrder,
) -> Result<Vec<Song>, AppError> {
    let p = lookup(playlists, pid)?;
    let sorted: Vec<Song> = match by {
        SortBy::Title => playlist::sort_by(p, lib, |s| s.title.to_lowercase()),
        SortBy::Artist => playlist::sort_by(p, lib, |s| s.artist.to_lowercase()),
        SortBy::Duration => playlist::sort_by(p, lib, |s| s.duration_sec),
        SortBy::AddedAt => playlist::sort_by(p, lib, |s| s.added_at),
    };
    Ok(match order {
        SortOrder::Asc => sorted,
        SortOrder::Desc => sorted.into_iter().rev().collect(),
    })
}

pub fn transform(
    lib: &Library,
    playlists: &Playlists,
    pid: &PlaylistId,
    op: TransformOp,
    n: Option<usize>,
) -> Result<Vec<Song>, AppError> {
    let p = lookup(playlists, pid)?;
    let derived = match op {
        TransformOp::Dedupe => playlist::dedupe(p),
        TransformOp::Reverse => playlist::reverse(p),
        TransformOp::Take => playlist::take(p, n.unwrap_or(0)),
        TransformOp::Drop => playlist::drop(p, n.unwrap_or(0)),
    };
    Ok(playlist::materialize_songs(&derived, lib))
}

pub fn materialize(lib: &Library, p: &Playlist) -> Vec<Song> {
    playlist::materialize_songs(p, lib)
}

// --- Helpers --------------------------------------------------------------

fn lookup<'a>(playlists: &'a Playlists, pid: &PlaylistId) -> Result<&'a Playlist, AppError> {
    playlists.get(pid).ok_or_else(|| {
        AppError::new(
            ErrorCode::PlaylistNotFound,
            format!("Playlist {pid} not found"),
        )
    })
}

fn map_playlist_err(e: PlaylistError) -> AppError {
    match e {
        PlaylistError::AlreadyInPlaylist => {
            AppError::new(ErrorCode::AlreadyInPlaylist, "Song already in playlist")
        }
        PlaylistError::SongNotInPlaylist => {
            AppError::new(ErrorCode::SongNotInPlaylist, "Song not in playlist")
        }
    }
}

fn criteria_predicate(c: &Criteria) -> Result<Box<dyn Fn(&Song) -> bool>, AppError> {
    use CriteriaField::*;
    use CriteriaOp::*;
    match (c.field, c.op) {
        (Title, Contains) => {
            let needle = c
                .value
                .clone()
                .ok_or_else(|| inv("value required"))?
                .to_lowercase();
            Ok(Box::new(move |s: &Song| {
                s.title.to_lowercase().contains(&needle)
            }))
        }
        (Artist, Contains) => {
            let needle = c
                .value
                .clone()
                .ok_or_else(|| inv("value required"))?
                .to_lowercase();
            Ok(Box::new(move |s: &Song| {
                s.artist.to_lowercase().contains(&needle)
            }))
        }
        (Genre, Equals) => {
            let v = c.value.clone().ok_or_else(|| inv("value required"))?;
            Ok(Box::new(move |s: &Song| s.genre == v))
        }
        (Duration, Range) => {
            let min = c.min.unwrap_or(0.0) as u32;
            let max = c.max.map(|v| v as u32).unwrap_or(u32::MAX);
            Ok(Box::new(move |s: &Song| {
                s.duration_sec >= min && s.duration_sec <= max
            }))
        }
        (AddedAt, Range) => {
            let min = c.min.map(|v| v as i64).unwrap_or(i64::MIN);
            let max = c.max.map(|v| v as i64).unwrap_or(i64::MAX);
            Ok(Box::new(move |s: &Song| {
                let t = s.added_at.timestamp();
                t >= min && t <= max
            }))
        }
        _ => Err(inv("Unsupported field/op combination")),
    }
}

fn inv(msg: &str) -> AppError {
    AppError::new(ErrorCode::InvalidCriteria, msg)
}
