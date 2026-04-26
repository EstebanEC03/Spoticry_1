use std::collections::HashMap;

use chrono::{DateTime, Utc};
use im::{OrdMap, Vector};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::song::{Song, SongId};

pub type PlaylistId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Playlist {
    pub id: PlaylistId,
    pub name: String,
    pub owner: String,
    pub song_ids: Vector<SongId>,
    pub created_at: DateTime<Utc>,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaylistError {
    AlreadyInPlaylist,
    SongNotInPlaylist,
}

impl Playlist {
    pub fn new(name: String, owner: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            owner,
            song_ids: Vector::new(),
            created_at: Utc::now(),
            version: 1,
        }
    }

    pub fn song_count(&self) -> usize {
        self.song_ids.len()
    }

    pub fn contains(&self, song_id: &SongId) -> bool {
        self.song_ids.iter().any(|id| id == song_id)
    }
}

// --- Pure operations. Each returns a brand new Playlist; never mutates input.

fn replace_ids(p: &Playlist, song_ids: Vector<SongId>) -> Playlist {
    Playlist {
        id: p.id.clone(),
        name: p.name.clone(),
        owner: p.owner.clone(),
        song_ids,
        created_at: p.created_at,
        version: p.version + 1,
    }
}

pub fn add_song(p: &Playlist, song_id: SongId) -> Result<Playlist, PlaylistError> {
    if p.contains(&song_id) {
        return Err(PlaylistError::AlreadyInPlaylist);
    }
    let next = p
        .song_ids
        .iter()
        .cloned()
        .chain(std::iter::once(song_id))
        .collect();
    Ok(replace_ids(p, next))
}

pub fn remove_song(p: &Playlist, song_id: &SongId) -> Result<Playlist, PlaylistError> {
    if !p.contains(song_id) {
        return Err(PlaylistError::SongNotInPlaylist);
    }
    let next = p
        .song_ids
        .iter()
        .filter(|id| id.as_str() != song_id.as_str())
        .cloned()
        .collect();
    Ok(replace_ids(p, next))
}

pub fn dedupe(p: &Playlist) -> Playlist {
    let next = p.song_ids.iter().fold(Vector::new(), |acc, id| {
        if acc.iter().any(|x| x == id) {
            acc
        } else {
            acc + Vector::unit(id.clone())
        }
    });
    replace_ids(p, next)
}

pub fn reverse(p: &Playlist) -> Playlist {
    let next = p.song_ids.iter().rev().cloned().collect();
    replace_ids(p, next)
}

pub fn take(p: &Playlist, n: usize) -> Playlist {
    let next = p.song_ids.iter().take(n).cloned().collect();
    replace_ids(p, next)
}

pub fn drop(p: &Playlist, n: usize) -> Playlist {
    let next = p.song_ids.iter().skip(n).cloned().collect();
    replace_ids(p, next)
}

// --- Derived views over library (no playlist mutation).

pub fn materialize_songs(p: &Playlist, lib: &HashMap<SongId, Song>) -> Vec<Song> {
    p.song_ids
        .iter()
        .filter_map(|id| lib.get(id))
        .cloned()
        .collect()
}

pub fn filter_by<F>(p: &Playlist, lib: &HashMap<SongId, Song>, pred: F) -> Vec<Song>
where
    F: Fn(&Song) -> bool,
{
    p.song_ids
        .iter()
        .filter_map(|id| lib.get(id))
        .filter(|s| pred(s))
        .cloned()
        .collect()
}

pub fn sort_by<K, F>(p: &Playlist, lib: &HashMap<SongId, Song>, key: F) -> Vec<Song>
where
    K: Ord + Clone,
    F: Fn(&Song) -> K,
{
    // Pure fold into an immutable OrdMap of buckets keyed by sort key. Stable
    // for ties (FIFO inside each bucket). No `let mut`, no in-place sort.
    materialize_songs(p, lib)
        .into_iter()
        .fold(OrdMap::<K, Vector<Song>>::new(), |acc, s| {
            let k = key(&s);
            let bucket = acc.get(&k).cloned().unwrap_or_default();
            acc.update(k, bucket + Vector::unit(s))
        })
        .into_iter()
        .flat_map(|(_, bucket)| bucket.into_iter())
        .collect()
}
