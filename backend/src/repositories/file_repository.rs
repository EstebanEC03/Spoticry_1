// JSON-on-disk persistence for library + playlists. Snapshots on shutdown,
// incremental saves on CLI mutation (caller's discretion). Atomic writes via
// temp + rename so a crash mid-save cannot corrupt the main file.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

use crate::domain::playlist::{Playlist, PlaylistId};
use crate::domain::song::{Song, SongId};

pub fn load_library<P: AsRef<Path>>(path: P) -> io::Result<HashMap<SongId, Song>> {
    load_map(path, |s: Song| (s.id.clone(), s))
}

pub fn save_library<P: AsRef<Path>>(
    path: P,
    library: &HashMap<SongId, Song>,
) -> io::Result<()> {
    save_values(path, library.values())
}

pub fn load_playlists<P: AsRef<Path>>(path: P) -> io::Result<HashMap<PlaylistId, Playlist>> {
    load_map(path, |p: Playlist| (p.id.clone(), p))
}

pub fn save_playlists<P: AsRef<Path>>(
    path: P,
    playlists: &HashMap<PlaylistId, Playlist>,
) -> io::Result<()> {
    save_values(path, playlists.values())
}

// --- Helpers --------------------------------------------------------------

fn load_map<P, T, F, K>(path: P, key_of: F) -> io::Result<HashMap<K, T>>
where
    P: AsRef<Path>,
    T: serde::de::DeserializeOwned,
    F: Fn(T) -> (K, T),
    K: std::hash::Hash + Eq,
{
    if !path.as_ref().exists() {
        return Ok(HashMap::new());
    }
    let data = fs::read_to_string(path)?;
    let items: Vec<T> = serde_json::from_str(&data).map_err(invalid_data)?;
    Ok(items.into_iter().map(key_of).collect())
}

fn save_values<'a, P, T, I>(path: P, values: I) -> io::Result<()>
where
    P: AsRef<Path>,
    T: serde::Serialize + 'a,
    I: IntoIterator<Item = &'a T>,
{
    let vec: Vec<&T> = values.into_iter().collect();
    let json = serde_json::to_string_pretty(&vec).map_err(invalid_data)?;
    write_atomic(path, json.as_bytes())
}

fn write_atomic<P: AsRef<Path>>(path: P, bytes: &[u8]) -> io::Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let mut tmp = path.to_path_buf();
    let stem = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("data.json");
    tmp.set_file_name(format!(".{stem}.tmp"));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn invalid_data(e: serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e)
}
