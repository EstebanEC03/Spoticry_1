use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub type SongId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Song {
    pub id: SongId,
    pub title: String,
    pub artist: String,
    pub genre: String,
    pub duration_sec: u32,
    pub bitrate: u32,
    pub added_at: DateTime<Utc>,
    pub path: PathBuf,
}
