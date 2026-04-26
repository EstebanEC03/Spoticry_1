// Top-level shell. Owns the WebSocket lifecycle, an immutable reducer-driven
// state tree (songs, playlists, view), and the audio player. Each view is a
// pure functional component receiving derived data + callbacks.

import React, { useEffect, useReducer, useState } from 'react';

import './App.css';
import * as api from './services/api.js';
import { connect } from './services/socket.js';
import { loadClientId, saveLocalPlaylists, syncPlaylists } from './services/storage.js';
import { usePlayer } from './hooks/usePlayer.js';
import { Sidebar } from './components/Sidebar.jsx';
import { LibraryView } from './components/LibraryView.jsx';
import { PlaylistsView } from './components/PlaylistsView.jsx';
import { Player } from './components/Player.jsx';

const initialState = {
  songs: [],
  playlists: [],
  view: 'library',
  status: 'connecting',
  error: null,
  toast: null,
};

const reducer = (state, action) => {
  switch (action.type) {
    case 'STATUS':
      return { ...state, status: action.status };
    case 'SONGS':
      return { ...state, songs: action.songs };
    case 'PLAYLISTS':
      return { ...state, playlists: action.playlists };
    case 'PLAYLIST_UPSERT': {
      const next = summarize(action.playlist);
      const existing = state.playlists.findIndex((p) => p.id === next.id);
      const playlists =
        existing >= 0
          ? state.playlists.map((p, i) => (i === existing ? next : p))
          : [...state.playlists, next];
      return { ...state, playlists };
    }
    case 'PLAYLIST_REMOVE':
      return { ...state, playlists: state.playlists.filter((p) => p.id !== action.id) };
    case 'VIEW':
      return { ...state, view: action.view };
    case 'ERROR':
      return { ...state, error: action.error };
    case 'TOAST':
      return { ...state, toast: action.toast };
    default:
      return state;
  }
};

const summarize = (p) => ({
  id: p.id,
  name: p.name,
  owner: p.owner,
  song_count: p.song_count ?? p.song_ids?.length ?? 0,
  song_ids: p.song_ids ?? [],
  created_at: p.created_at,
  version: p.version ?? 1,
});

export default function App() {
  const [state, dispatch] = useReducer(reducer, initialState);
  const [socket, setSocket] = useState(null);
  const [hydrated, setHydrated] = useState(false);
  const player = usePlayer(socket);

  // Connect once per real mount. In React StrictMode dev this effect runs
  // twice (mount → cleanup → mount); the cleanup closes the first socket
  // and the second mount establishes the live one. Production has no
  // StrictMode, so it runs exactly once.
  useEffect(() => {
    const url = `ws://${window.location.hostname}:7878/`;
    const sock = connect(url, {
      onStatus: (s) => dispatch({ type: 'STATUS', status: s }),
    });
    setSocket(sock);

    let cancelled = false;
    (async () => {
      try {
        await sock.ready();
        if (cancelled) return;
        await api.hello(sock, loadClientId(), '1.0');
        if (cancelled) return;
        const songsData = await api.listSongs(sock);
        if (cancelled) return;
        dispatch({ type: 'SONGS', songs: songsData.songs ?? [] });
        const playlists = await syncPlaylists(sock);
        if (cancelled) return;
        dispatch({ type: 'PLAYLISTS', playlists: playlists.map(summarize) });
        setHydrated(true);
      } catch (e) {
        if (cancelled) return;
        console.error('bootstrap failed', e);
        dispatch({ type: 'ERROR', error: e.message });
      }
    })();

    return () => {
      cancelled = true;
      sock.close();
    };
  }, []);

  // Persist playlist summaries locally on every change. Skip until bootstrap
  // finished — otherwise the initial empty state would clobber whatever was
  // in localStorage before the sync had a chance to merge it.
  useEffect(() => {
    if (!hydrated) return;
    saveLocalPlaylists(state.playlists);
  }, [hydrated, state.playlists]);

  // Server-pushed library refresh on CLI add/remove.
  useEffect(() => {
    if (!socket) return undefined;
    return socket.onNotify(async (msg) => {
      if (msg.type !== 'LIBRARY_UPDATED') return;
      try {
        const data = await api.listSongs(socket);
        dispatch({ type: 'SONGS', songs: data.songs ?? [] });
      } catch (e) {
        console.warn('LIBRARY_UPDATED refresh failed', e);
      }
    });
  }, [socket]);

  const refreshSongs = async () => {
    if (!socket) return;
    try {
      const data = await api.listSongs(socket);
      dispatch({ type: 'SONGS', songs: data.songs ?? [] });
    } catch (e) {
      console.error('refreshSongs failed', e);
    }
  };

  const refreshPlaylists = async () => {
    if (!socket) return;
    try {
      const data = await api.listPlaylists(socket);
      dispatch({ type: 'PLAYLISTS', playlists: (data.playlists ?? []).map(summarize) });
    } catch (e) {
      console.error('refreshPlaylists failed', e);
    }
  };

  const handleCreatePlaylist = async (name) => {
    if (!socket) return;
    try {
      const data = await api.createPlaylist(socket, name, loadClientId());
      dispatch({ type: 'PLAYLIST_UPSERT', playlist: data.playlist });
      dispatch({ type: 'TOAST', toast: `Created "${data.playlist.name}"` });
    } catch (e) {
      dispatch({ type: 'TOAST', toast: `Create failed: ${e.message}` });
    }
  };

  const handleDeletePlaylist = async (id) => {
    if (!socket) return;
    try {
      await api.deletePlaylist(socket, id);
      dispatch({ type: 'PLAYLIST_REMOVE', id });
      dispatch({ type: 'TOAST', toast: 'Playlist deleted' });
    } catch (e) {
      dispatch({ type: 'TOAST', toast: `Delete failed: ${e.message}` });
    }
  };

  const handleAddToPlaylist = async (playlistId, songId) => {
    if (!socket) return;
    try {
      const data = await api.addSongToPlaylist(socket, playlistId, songId);
      dispatch({ type: 'PLAYLIST_UPSERT', playlist: data.playlist });
      dispatch({ type: 'TOAST', toast: 'Added to playlist' });
    } catch (e) {
      dispatch({ type: 'TOAST', toast: `Add failed: ${e.message}` });
    }
  };

  const handlePlaylistMutated = (playlist) => {
    dispatch({ type: 'PLAYLIST_UPSERT', playlist });
  };

  const handlePlay = async (song, queue, index) => {
    try {
      await player.play(song, queue, index);
    } catch (e) {
      dispatch({ type: 'TOAST', toast: `Play failed: ${e.message}` });
    }
  };

  const renderView = () => {
    switch (state.view) {
      case 'playlists':
        return (
          <PlaylistsView
            socket={socket}
            playlists={state.playlists}
            onCreate={handleCreatePlaylist}
            onDelete={handleDeletePlaylist}
            onPlay={handlePlay}
            onPlaylistMutated={handlePlaylistMutated}
          />
        );
      case 'now-playing':
        return <NowPlayingView player={player} />;
      case 'library':
      default:
        return (
          <LibraryView
            socket={socket}
            songs={state.songs}
            playlists={state.playlists}
            onPlay={handlePlay}
            onAddToPlaylist={handleAddToPlaylist}
          />
        );
    }
  };

  return (
    <div className="bg-background text-on-surface h-screen flex">
      <Sidebar
        view={state.view}
        onViewChange={(v) => dispatch({ type: 'VIEW', view: v })}
        onCreatePlaylist={() => dispatch({ type: 'VIEW', view: 'playlists' })}
        status={state.status}
      />

      <main className="flex-1 flex flex-col relative w-full md:ml-[280px] h-screen overflow-y-auto pb-[96px]">
        <header className="bg-[#121212]/80 backdrop-blur-xl h-16 border-b border-white/5 flex justify-between items-center px-6 w-full z-30 sticky top-0">
          <div className="md:hidden text-xl font-black italic tracking-tighter text-[#8B5CF6]">SpotiCry</div>
          <div className="text-on-surface-variant text-sm hidden md:block">
            {state.songs.length} songs · {state.playlists.length} playlists
          </div>
          <div className="flex items-center gap-3 text-on-surface-variant text-xs">
            <button
              type="button"
              onClick={refreshSongs}
              className="hover:text-white flex items-center gap-1"
              title="Refresh library from server"
            >
              <span className="material-symbols-outlined text-[18px]">refresh</span> songs
            </button>
            <button
              type="button"
              onClick={refreshPlaylists}
              className="hover:text-white flex items-center gap-1"
              title="Refresh playlists from server"
            >
              <span className="material-symbols-outlined text-[18px]">sync</span> playlists
            </button>
          </div>
        </header>

        {state.error && (
          <div className="bg-error/10 border-l-4 border-error text-error px-4 py-2 text-sm">
            {state.error}
          </div>
        )}

        {renderView()}

        {state.toast && (
          <Toast message={state.toast} onClose={() => dispatch({ type: 'TOAST', toast: null })} />
        )}
      </main>

      <Player player={player} />
    </div>
  );
}

const formatTime = (sec) => {
  const s = Math.max(0, Math.floor(sec || 0));
  const m = Math.floor(s / 60);
  const r = s % 60;
  return `${m}:${r.toString().padStart(2, '0')}`;
};

const NowPlayingView = ({ player }) => {
  const { currentSong, currentTime, duration, isPlaying } = player;
  if (!currentSong) {
    return (
      <div className="p-gutter lg:p-10 flex items-center justify-center h-full text-on-surface-variant">
        Pick a song from the Library to start streaming.
      </div>
    );
  }
  const total = duration || currentSong.duration_sec || 0;
  return (
    <div className="p-gutter lg:p-10 flex flex-col items-center gap-6">
      <div className="w-72 h-72 rounded-xl shadow-2xl bg-gradient-to-br from-violet-500/40 via-purple-700/30 to-blue-500/30 flex items-center justify-center">
        <span className="material-symbols-outlined text-[120px] text-white/30">music_note</span>
      </div>
      <div className="text-center">
        <h1 className="text-display-lg">{currentSong.title}</h1>
        <p className="text-on-surface-variant text-base mt-2">
          {currentSong.artist} · {currentSong.genre}
        </p>
      </div>
      <div className="text-sm text-on-surface-variant font-medium tracking-wide">
        {isPlaying ? 'Playing' : 'Paused'} · {formatTime(currentTime)} / {formatTime(total)}
      </div>
    </div>
  );
};

const Toast = ({ message, onClose }) => {
  useEffect(() => {
    const t = setTimeout(onClose, 3000);
    return () => clearTimeout(t);
  }, [message, onClose]);
  return (
    <div className="fixed bottom-[110px] right-6 bg-surface-container-high border border-outline-variant rounded-lg px-4 py-2 text-sm shadow-2xl z-50">
      {message}
    </div>
  );
};
