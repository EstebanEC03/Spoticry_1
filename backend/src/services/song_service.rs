// Song service. Imperative — owns library mutations and stream-aware checks.
// Pure search/filter helpers stay folded in here too; keeping them next to the
// state access avoids needless type ceremony at the controller layer.

use std::path::Path;

use chrono::Utc;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, MetadataRevision, StandardTagKey};
use symphonia::core::probe::Hint;
use uuid::Uuid;

use crate::domain::playlist as playlist_dom;
use crate::domain::song::{Song, SongId};
use crate::error::AppError;
use crate::network::protocol::{Criteria, CriteriaField, CriteriaOp, ErrorCode};
use crate::states::app_state::{AppState, Notification};

// --- Queries --------------------------------------------------------------

pub fn list_songs(state: &AppState, criteria: Option<&Criteria>) -> Result<Vec<Song>, AppError> {
    let lib = state.library.read().map_err(|_| pois("library"))?;
    let all: Vec<Song> = lib.values().cloned().collect();
    drop(lib);
    match criteria {
        None => Ok(all),
        Some(c) => apply_criteria(all, c),
    }
}

pub fn get_song(state: &AppState, song_id: &SongId) -> Result<Song, AppError> {
    let lib = state.library.read().map_err(|_| pois("library"))?;
    lib.get(song_id)
        .cloned()
        .ok_or_else(|| AppError::new(ErrorCode::NotFound, format!("Song {song_id} not found")))
}

pub fn apply_criteria(songs: Vec<Song>, c: &Criteria) -> Result<Vec<Song>, AppError> {
    use CriteriaField::*;
    use CriteriaOp::*;
    match (c.field, c.op) {
        (Title, Contains) => {
            let needle = c
                .value
                .as_deref()
                .ok_or_else(invalid_value)?
                .to_lowercase();
            Ok(songs
                .into_iter()
                .filter(|s| s.title.to_lowercase().contains(&needle))
                .collect())
        }
        (Artist, Contains) => {
            let needle = c
                .value
                .as_deref()
                .ok_or_else(invalid_value)?
                .to_lowercase();
            Ok(songs
                .into_iter()
                .filter(|s| s.artist.to_lowercase().contains(&needle))
                .collect())
        }
        (Genre, Equals) => {
            let v = c.value.as_deref().ok_or_else(invalid_value)?.to_string();
            Ok(songs.into_iter().filter(|s| s.genre == v).collect())
        }
        (Duration, Range) => {
            let min = c.min.unwrap_or(0.0) as u32;
            let max = c.max.map(|v| v as u32).unwrap_or(u32::MAX);
            Ok(songs
                .into_iter()
                .filter(|s| s.duration_sec >= min && s.duration_sec <= max)
                .collect())
        }
        (AddedAt, Range) => {
            let min = c.min.map(|v| v as i64).unwrap_or(i64::MIN);
            let max = c.max.map(|v| v as i64).unwrap_or(i64::MAX);
            Ok(songs
                .into_iter()
                .filter(|s| {
                    let ts = s.added_at.timestamp();
                    ts >= min && ts <= max
                })
                .collect())
        }
        _ => Err(AppError::new(
            ErrorCode::InvalidCriteria,
            "Unsupported field/op combination",
        )),
    }
}

// --- CLI mutations --------------------------------------------------------

#[derive(Debug, Default, Clone)]
pub struct SongOverrides {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub genre: Option<String>,
}

pub fn add_from_path(
    state: &AppState,
    path: &Path,
    overrides: SongOverrides,
) -> Result<Song, AppError> {
    if !path.exists() {
        return Err(AppError::new(
            ErrorCode::FileNotFound,
            format!("Source file does not exist: {}", path.display()),
        ));
    }
    if !is_mp3(path) {
        return Err(AppError::new(ErrorCode::UnsupportedFormat, "Only MP3 supported"));
    }
    let canonical = path
        .canonicalize()
        .map_err(|e| AppError::new(ErrorCode::FileNotFound, format!("Cannot canonicalize: {e}")))?;

    {
        let lib = state.library.read().map_err(|_| pois("library"))?;
        if lib.values().any(|s| s.path == canonical) {
            return Err(AppError::new(
                ErrorCode::DuplicateSong,
                "Song already in library",
            ));
        }
    }

    // Strict: file must be a decodable MP3 stream. Symphonia's probe doubles
    // as content validation — extension alone (is_mp3) doesn't catch a JPG
    // renamed to .mp3, but the demuxer does.
    let probed = probe_metadata(&canonical).ok_or_else(|| {
        AppError::new(
            ErrorCode::UnsupportedFormat,
            "File is not a decodable MP3 stream",
        )
    })?;
    let duration_sec = probed.duration_sec.ok_or_else(|| {
        AppError::new(
            ErrorCode::UnsupportedFormat,
            "Cannot determine MP3 duration (corrupt or empty stream)",
        )
    })?;
    let file_size = std::fs::metadata(&canonical).map(|m| m.len()).unwrap_or(0);
    let bitrate = if duration_sec > 0 {
        (file_size * 8 / duration_sec as u64) as u32
    } else {
        0
    };

    let stem = canonical
        .file_stem()
        .and_then(|s| s.to_str())
        .map(String::from);
    let title = overrides
        .title
        .or(probed.title)
        .or(stem)
        .unwrap_or_else(|| "Untitled".to_string());
    let artist = overrides
        .artist
        .or(probed.artist)
        .unwrap_or_else(|| "Unknown".to_string());
    let genre = overrides
        .genre
        .or(probed.genre)
        .unwrap_or_else(|| "Unknown".to_string());

    let song = Song {
        id: Uuid::new_v4().simple().to_string(),
        title,
        artist,
        genre,
        duration_sec,
        bitrate,
        added_at: Utc::now(),
        path: canonical,
    };

    let song = {
        let mut lib = state.library.write().map_err(|_| pois("library"))?;
        lib.insert(song.id.clone(), song.clone());
        song
    };
    state.notify(Notification::LibraryUpdated {
        event: "added",
        song_id: song.id.clone(),
    });
    Ok(song)
}

pub fn remove_song(state: &AppState, song_id: &SongId) -> Result<Song, AppError> {
    if state.song_in_use(song_id) {
        return Err(AppError::new(
            ErrorCode::SongInUse,
            "Cannot remove song: currently streaming",
        ));
    }
    let removed = {
        let mut lib = state.library.write().map_err(|_| pois("library"))?;
        lib.remove(song_id)
    };
    let song = removed
        .ok_or_else(|| AppError::new(ErrorCode::NotFound, format!("Song {song_id} not found")))?;

    // Cascade: drop the song id from any playlist that referenced it. Without
    // this, playlists keep dangling SongIds forever (materialize_songs hides
    // them, but playlists.json grows and `version` lies about contents).
    let cascaded = {
        let mut playlists = state.playlists.write().map_err(|_| pois("playlists"))?;
        let updates: Vec<(_, _)> = playlists
            .iter()
            .filter(|(_, p)| p.contains(song_id))
            .filter_map(|(pid, p)| {
                playlist_dom::remove_song(p, song_id)
                    .ok()
                    .map(|np| (pid.clone(), np))
            })
            .collect();
        let touched = !updates.is_empty();
        for (pid, np) in updates {
            playlists.insert(pid, np);
        }
        touched
    };
    if cascaded {
        state.persist_playlists();
    }

    state.notify(Notification::LibraryUpdated {
        event: "removed",
        song_id: song.id.clone(),
    });
    Ok(song)
}

// --- Helpers --------------------------------------------------------------

fn is_mp3(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("mp3"))
        .unwrap_or(false)
}

#[derive(Debug, Default)]
struct ProbedMeta {
    duration_sec: Option<u32>,
    title: Option<String>,
    artist: Option<String>,
    genre: Option<String>,
}

fn probe_metadata(path: &Path) -> Option<ProbedMeta> {
    let file = std::fs::File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("mp3");
    let mut probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()?;

    let duration_sec = probed
        .format
        .tracks()
        .first()
        .and_then(|t| {
            let p = &t.codec_params;
            let sr = p.sample_rate? as u64;
            let frames = p.n_frames?;
            if sr == 0 { None } else { Some((frames / sr) as u32) }
        });

    // ID3v2 lives in the format reader's metadata log; ID3v1 surfaces via the
    // probe-level metadata. Walk both, format reader first (richer source).
    let mut meta = ProbedMeta {
        duration_sec,
        ..Default::default()
    };
    if let Some(rev) = probed.format.metadata().current() {
        merge_tags(&mut meta, rev);
    }
    if let Some(rev) = probed.metadata.get().as_ref().and_then(|m| m.current()) {
        merge_tags(&mut meta, rev);
    }
    Some(meta)
}

fn merge_tags(meta: &mut ProbedMeta, rev: &MetadataRevision) {
    for tag in rev.tags() {
        let Some(key) = tag.std_key else { continue };
        let value = tag.value.to_string();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let slot = match key {
            StandardTagKey::TrackTitle => &mut meta.title,
            StandardTagKey::Artist => &mut meta.artist,
            StandardTagKey::Genre => &mut meta.genre,
            _ => continue,
        };
        if slot.is_none() {
            *slot = Some(trimmed.to_string());
        }
    }
}

fn pois(what: &str) -> AppError {
    AppError::new(ErrorCode::ServerError, format!("{what} lock poisoned"))
}

fn invalid_value() -> AppError {
    AppError::new(ErrorCode::InvalidCriteria, "value required")
}
