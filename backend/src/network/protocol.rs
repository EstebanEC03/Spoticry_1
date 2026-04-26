// Wire protocol for SpotiCry. JSON, one object per line, serde-tagged on "type".
// Mirrors the contract in SERVER_API.md — if a field moves here, update that doc
// in the same commit.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::playlist::{Playlist, PlaylistId};
use crate::domain::song::{Song, SongId};

// --- Request envelope ------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RequestEnvelope {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(flatten)]
    pub payload: RequestPayload,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum RequestPayload {
    #[serde(rename = "HELLO")]
    Hello { client_id: String, version: String },

    #[serde(rename = "PING")]
    Ping {
        #[serde(default)]
        ts: Option<i64>,
    },

    #[serde(rename = "LIST_SONGS")]
    ListSongs {
        #[serde(default)]
        criteria: Option<Criteria>,
    },

    #[serde(rename = "GET_SONG")]
    GetSong { song_id: SongId },

    #[serde(rename = "PLAY_SONG")]
    PlaySong { song_id: SongId },

    #[serde(rename = "STOP_SONG")]
    StopSong { stream_id: String },

    #[serde(rename = "CREATE_PLAYLIST")]
    CreatePlaylist { name: String, owner: String },

    #[serde(rename = "DELETE_PLAYLIST")]
    DeletePlaylist { playlist_id: PlaylistId },

    #[serde(rename = "LIST_PLAYLISTS")]
    ListPlaylists {
        #[serde(default)]
        owner: Option<String>,
    },

    #[serde(rename = "GET_PLAYLIST")]
    GetPlaylist { playlist_id: PlaylistId },

    #[serde(rename = "ADD_SONG_TO_PLAYLIST")]
    AddSongToPlaylist {
        playlist_id: PlaylistId,
        song_id: SongId,
    },

    #[serde(rename = "REMOVE_SONG_FROM_PLAYLIST")]
    RemoveSongFromPlaylist {
        playlist_id: PlaylistId,
        song_id: SongId,
    },

    #[serde(rename = "FILTER_PLAYLIST")]
    FilterPlaylist {
        playlist_id: PlaylistId,
        criteria: Criteria,
    },

    #[serde(rename = "SORT_PLAYLIST")]
    SortPlaylist {
        playlist_id: PlaylistId,
        by: SortBy,
        #[serde(default)]
        order: SortOrder,
    },

    #[serde(rename = "TRANSFORM_PLAYLIST")]
    TransformPlaylist {
        playlist_id: PlaylistId,
        op: TransformOp,
        #[serde(default)]
        n: Option<usize>,
    },
}

// --- Shared request sub-types ---------------------------------------------

#[derive(Debug, Deserialize, Clone)]
pub struct Criteria {
    pub field: CriteriaField,
    pub op: CriteriaOp,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum CriteriaField {
    Title,
    Artist,
    Genre,
    Duration,
    AddedAt,
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum CriteriaOp {
    Contains,
    Equals,
    Range,
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum SortBy {
    Title,
    Artist,
    Duration,
    AddedAt,
}

#[derive(Debug, Deserialize, Clone, Copy, Default)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    #[default]
    Asc,
    Desc,
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum TransformOp {
    Dedupe,
    Reverse,
    Take,
    Drop,
}

// --- Response envelope -----------------------------------------------------

#[derive(Debug, Serialize)]
pub struct Response {
    pub id: String,
    #[serde(rename = "type")]
    pub resp_type: String,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ResponseError>,
}

#[derive(Debug, Serialize)]
pub struct ResponseError {
    pub code: ErrorCode,
    pub message: String,
}

impl Response {
    pub fn ok(id: String, resp_type: &str, data: Value) -> Self {
        Self {
            id,
            resp_type: resp_type.to_string(),
            status: "ok",
            data: Some(data),
            error: None,
        }
    }

    pub fn err(id: String, resp_type: &str, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            id,
            resp_type: resp_type.to_string(),
            status: "error",
            data: None,
            error: Some(ResponseError {
                code,
                message: message.into(),
            }),
        }
    }
}

// --- Error codes -----------------------------------------------------------

// Full protocol catalogue per SERVER_API.md §4. Some variants are reserved
// for future paths the wire format mandates but the current codebase does
// not yet trigger (e.g. UnknownType — serde already rejects unknown `type`
// values as InvalidRequest before we'd emit it).
#[allow(dead_code)]
#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    #[serde(rename = "INVALID_REQUEST")]
    InvalidRequest,
    #[serde(rename = "UNKNOWN_TYPE")]
    UnknownType,
    #[serde(rename = "INVALID_CRITERIA")]
    InvalidCriteria,
    #[serde(rename = "NOT_FOUND")]
    NotFound,
    #[serde(rename = "PLAYLIST_NOT_FOUND")]
    PlaylistNotFound,
    #[serde(rename = "SONG_NOT_FOUND")]
    SongNotFound,
    #[serde(rename = "DUPLICATE_NAME")]
    DuplicateName,
    #[serde(rename = "DUPLICATE_SONG")]
    DuplicateSong,
    #[serde(rename = "ALREADY_IN_PLAYLIST")]
    AlreadyInPlaylist,
    #[serde(rename = "SONG_NOT_IN_PLAYLIST")]
    SongNotInPlaylist,
    #[serde(rename = "SONG_IN_USE")]
    SongInUse,
    #[serde(rename = "ALREADY_PLAYING")]
    AlreadyPlaying,
    #[serde(rename = "STREAM_NOT_FOUND")]
    StreamNotFound,
    #[serde(rename = "UNSUPPORTED_FORMAT")]
    UnsupportedFormat,
    #[serde(rename = "FILE_NOT_FOUND")]
    FileNotFound,
    #[serde(rename = "UNSUPPORTED_OP")]
    UnsupportedOp,
    #[serde(rename = "VERSION_MISMATCH")]
    VersionMismatch,
    #[serde(rename = "SERVER_ERROR")]
    ServerError,
}

// --- Response DTOs ---------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SongDto {
    pub id: SongId,
    pub title: String,
    pub artist: String,
    pub genre: String,
    pub duration_sec: u32,
    pub bitrate: u32,
    pub added_at: DateTime<Utc>,
}

impl From<&Song> for SongDto {
    fn from(s: &Song) -> Self {
        Self {
            id: s.id.clone(),
            title: s.title.clone(),
            artist: s.artist.clone(),
            genre: s.genre.clone(),
            duration_sec: s.duration_sec,
            bitrate: s.bitrate,
            added_at: s.added_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PlaylistDto {
    pub id: PlaylistId,
    pub name: String,
    pub owner: String,
    pub song_ids: Vec<SongId>,
    pub created_at: DateTime<Utc>,
    pub version: u64,
}

impl From<&Playlist> for PlaylistDto {
    fn from(p: &Playlist) -> Self {
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            owner: p.owner.clone(),
            song_ids: p.song_ids.iter().cloned().collect(),
            created_at: p.created_at,
            version: p.version,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PlaylistSummaryDto {
    pub id: PlaylistId,
    pub name: String,
    pub owner: String,
    pub song_count: usize,
    pub created_at: DateTime<Utc>,
}

impl From<&Playlist> for PlaylistSummaryDto {
    fn from(p: &Playlist) -> Self {
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            owner: p.owner.clone(),
            song_count: p.song_count(),
            created_at: p.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PlaylistDetailDto {
    pub id: PlaylistId,
    pub name: String,
    pub owner: String,
    pub song_ids: Vec<SongId>,
    pub songs: Vec<SongDto>,
    pub created_at: DateTime<Utc>,
    pub version: u64,
}

// --- Stream chunk (server → client, unsolicited) --------------------------

#[derive(Debug, Serialize)]
pub struct StreamChunk {
    #[serde(rename = "type")]
    pub msg_type: &'static str,
    pub stream_id: String,
    pub seq: u64,
    pub payload_b64: String,
    pub eof: bool,
}

impl StreamChunk {
    pub fn new(stream_id: String, seq: u64, payload_b64: String, eof: bool) -> Self {
        Self {
            msg_type: "STREAM_CHUNK",
            stream_id,
            seq,
            payload_b64,
            eof,
        }
    }
}
