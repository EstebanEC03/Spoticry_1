use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use serde::Serialize;
use tokio::sync::{broadcast, oneshot};

use crate::domain::playlist::{Playlist, PlaylistId};
use crate::domain::song::{Song, SongId};
use crate::repositories::file_repository;

pub type StreamId = String;

const NOTIFICATION_BUFFER: usize = 64;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type")]
pub enum Notification {
    #[serde(rename = "LIBRARY_UPDATED")]
    LibraryUpdated {
        event: &'static str,
        song_id: SongId,
    },
}

// Held in `active_streams`. Dropping the StreamHandle drops the cancel sender,
// which signals the matching pump task (its `cancel_rx.await` returns Err) to
// stop sending chunks.
#[allow(dead_code)]
pub struct StreamHandle {
    pub song_id: SongId,
    cancel: oneshot::Sender<()>,
}

#[derive(Clone)]
pub struct AppState {
    pub library: Arc<RwLock<HashMap<SongId, Song>>>,
    pub playlists: Arc<RwLock<HashMap<PlaylistId, Playlist>>>,
    pub active_streams: Arc<Mutex<HashMap<StreamId, StreamHandle>>>,
    pub notifications: broadcast::Sender<Notification>,
    pub library_path: Arc<PathBuf>,
    pub playlists_path: Arc<PathBuf>,
}

impl AppState {
    pub fn new() -> Self {
        Self::with_data(
            HashMap::new(),
            HashMap::new(),
            PathBuf::new(),
            PathBuf::new(),
        )
    }

    pub fn with_data(
        library: HashMap<SongId, Song>,
        playlists: HashMap<PlaylistId, Playlist>,
        library_path: PathBuf,
        playlists_path: PathBuf,
    ) -> Self {
        let (tx, _) = broadcast::channel(NOTIFICATION_BUFFER);
        Self {
            library: Arc::new(RwLock::new(library)),
            playlists: Arc::new(RwLock::new(playlists)),
            active_streams: Arc::new(Mutex::new(HashMap::new())),
            notifications: tx,
            library_path: Arc::new(library_path),
            playlists_path: Arc::new(playlists_path),
        }
    }

    pub fn persist_library(&self) {
        if self.library_path.as_os_str().is_empty() {
            return;
        }
        let snap = self.snapshot_library();
        if let Err(e) = file_repository::save_library(self.library_path.as_ref(), &snap) {
            eprintln!("WARN  save_library failed: {e}");
        }
    }

    pub fn persist_playlists(&self) {
        if self.playlists_path.as_os_str().is_empty() {
            return;
        }
        let snap = self.snapshot_playlists();
        if let Err(e) = file_repository::save_playlists(self.playlists_path.as_ref(), &snap) {
            eprintln!("WARN  save_playlists failed: {e}");
        }
    }

    pub fn notify(&self, n: Notification) {
        // Err(_) means no active subscribers — broadcast is fire-and-forget.
        let _ = self.notifications.send(n);
    }

    pub fn song_in_use(&self, song_id: &SongId) -> bool {
        self.active_streams
            .lock()
            .map(|s| s.values().any(|h| &h.song_id == song_id))
            .unwrap_or(false)
    }

    pub fn register_stream(
        &self,
        stream_id: StreamId,
        song_id: SongId,
        cancel: oneshot::Sender<()>,
    ) -> bool {
        match self.active_streams.lock() {
            Ok(mut guard) => {
                guard.insert(stream_id, StreamHandle { song_id, cancel });
                true
            }
            Err(_) => false,
        }
    }

    pub fn unregister_stream(&self, stream_id: &str) -> Option<SongId> {
        self.active_streams
            .lock()
            .ok()
            .and_then(|mut guard| guard.remove(stream_id))
            .map(|h| h.song_id)
    }

    pub fn snapshot_library(&self) -> HashMap<SongId, Song> {
        self.library.read().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn snapshot_playlists(&self) -> HashMap<PlaylistId, Playlist> {
        self.playlists.read().map(|g| g.clone()).unwrap_or_default()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
