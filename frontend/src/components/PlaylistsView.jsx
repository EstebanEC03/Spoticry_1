// Playlists view — list, detail, and the four "transform" actions backed by
// the server-side functional playlist module. The detail view lets the user
// drive sort / filter / dedupe / reverse / take / drop and shows the result
// without mutating the source playlist (server returns derived song lists).

import React, { useEffect, useMemo, useState } from 'react';

import {
  filterPlaylist,
  genreEquals,
  removeSongFromPlaylist,
  sortPlaylist,
  transformPlaylist,
  getPlaylist,
  deletePlaylist,
} from '../services/api.js';

const formatDuration = (sec) => {
  const s = Math.max(0, Math.floor(sec || 0));
  const m = Math.floor(s / 60);
  const r = s % 60;
  return `${m}:${r.toString().padStart(2, '0')}`;
};

export const PlaylistsView = ({ socket, playlists, onCreate, onDelete, onPlay, onPlaylistMutated }) => {
  const [selectedId, setSelectedId] = useState(null);
  const [name, setName] = useState('');

  const selected = useMemo(
    () => playlists.find((p) => p.id === selectedId),
    [playlists, selectedId]
  );

  if (selectedId && selected) {
    return (
      <PlaylistDetail
        socket={socket}
        playlist={selected}
        onBack={() => setSelectedId(null)}
        onDelete={async (id) => {
          await onDelete(id);
          setSelectedId(null);
        }}
        onPlay={onPlay}
        onMutated={onPlaylistMutated}
      />
    );
  }

  return (
    <div className="p-gutter lg:p-10 flex flex-col gap-8">
      <header className="flex flex-col gap-2">
        <span className="text-[#8B5CF6] text-xs uppercase tracking-widest font-semibold">Your library</span>
        <h1 className="text-display-lg">Playlists</h1>
      </header>

      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (name.trim().length === 0) return;
          onCreate(name.trim());
          setName('');
        }}
        className="flex items-center gap-3 bg-surface-container/60 backdrop-blur rounded-xl p-4 border border-white/5"
      >
        <span className="material-symbols-outlined text-outline">playlist_add</span>
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Playlist name"
          className="flex-1 bg-transparent border-none focus:outline-none focus:ring-0 text-on-surface placeholder-outline"
        />
        <button
          type="submit"
          className="bg-primary hover:bg-inverse-primary text-on-primary text-xs uppercase tracking-wider font-semibold py-2 px-5 rounded-full transition-all"
        >
          Create
        </button>
      </form>

      {playlists.length === 0 ? (
        <div className="py-12 text-center text-on-surface-variant">No playlists yet.</div>
      ) : (
        <div className="grid grid-cols-2 md:grid-cols-4 gap-gutter">
          {playlists.map((p) => (
            <button
              key={p.id}
              type="button"
              onClick={() => setSelectedId(p.id)}
              className="group relative bg-surface-container rounded-lg p-4 hover:bg-surface-container-high transition-colors duration-200 border border-transparent hover:border-outline-variant text-left"
            >
              <div className="relative w-full aspect-square rounded-md overflow-hidden mb-4 shadow-lg bg-gradient-to-br from-violet-500/40 via-purple-700/30 to-blue-500/30 flex items-center justify-center">
                <span className="material-symbols-outlined text-[64px] text-white/30">queue_music</span>
              </div>
              <h3 className="text-white truncate text-base mb-1">{p.name}</h3>
              <p className="text-on-surface-variant text-sm truncate">
                {p.song_count ?? p.song_ids?.length ?? 0} songs · v{p.version ?? 1}
              </p>
            </button>
          ))}
        </div>
      )}
    </div>
  );
};

const PlaylistDetail = ({ socket, playlist, onBack, onDelete, onPlay, onMutated }) => {
  const [detail, setDetail] = useState(null);
  const [derived, setDerived] = useState(null); // {label, songs}
  const [sortBy, setSortBy] = useState('title');
  const [sortOrder, setSortOrder] = useState('asc');
  const [filterGenre, setFilterGenre] = useState('');
  const [transformN, setTransformN] = useState(5);
  const [error, setError] = useState(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const data = await getPlaylist(socket, playlist.id);
        if (!cancelled) setDetail(data.playlist);
      } catch (e) {
        if (!cancelled) setError(e.message);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [socket, playlist.id, playlist.version]);

  const baseSongs = detail?.songs ?? [];
  const visible = derived?.songs ?? baseSongs;

  const handleSort = async () => {
    try {
      const data = await sortPlaylist(socket, playlist.id, sortBy, sortOrder);
      setDerived({ label: `Sort: ${sortBy} ${sortOrder}`, songs: data.sorted_songs });
    } catch (e) {
      setError(e.message);
    }
  };

  const handleFilter = async () => {
    if (!filterGenre.trim()) return;
    try {
      const data = await filterPlaylist(socket, playlist.id, genreEquals(filterGenre.trim()));
      setDerived({ label: `Filter: genre = ${filterGenre.trim()}`, songs: data.filtered_songs });
    } catch (e) {
      setError(e.message);
    }
  };

  const handleTransform = async (op, n) => {
    try {
      const data = await transformPlaylist(socket, playlist.id, op, n);
      setDerived({ label: `Transform: ${op}${n != null ? ` ${n}` : ''}`, songs: data.result_songs });
    } catch (e) {
      setError(e.message);
    }
  };

  const removeSong = async (songId) => {
    try {
      const data = await removeSongFromPlaylist(socket, playlist.id, songId);
      onMutated?.(data.playlist);
      // Refresh detail
      const fresh = await getPlaylist(socket, playlist.id);
      setDetail(fresh.playlist);
      setDerived(null);
    } catch (e) {
      setError(e.message);
    }
  };

  return (
    <div className="p-gutter lg:p-10 flex flex-col gap-6">
      <button
        type="button"
        onClick={onBack}
        className="self-start flex items-center gap-1 text-sm text-on-surface-variant hover:text-white"
      >
        <span className="material-symbols-outlined text-[18px]">arrow_back</span> back to playlists
      </button>

      <header className="flex items-end justify-between gap-4">
        <div>
          <span className="text-[#8B5CF6] text-xs uppercase tracking-widest font-semibold">Playlist</span>
          <h1 className="text-display-lg">{playlist.name}</h1>
          <p className="text-on-surface-variant text-sm">
            owner: {playlist.owner} · v{detail?.version ?? playlist.version} · {baseSongs.length} songs
          </p>
        </div>
        <button
          type="button"
          onClick={() => onDelete(playlist.id)}
          className="text-error text-sm border border-error/30 hover:bg-error/10 rounded-full px-4 py-2"
        >
          Delete
        </button>
      </header>

      <section className="bg-surface-container/60 backdrop-blur rounded-xl p-4 border border-white/5 grid grid-cols-1 md:grid-cols-3 gap-4">
        <div className="flex items-center gap-2">
          <span className="text-xs uppercase tracking-wider text-on-surface-variant">Sort</span>
          <select value={sortBy} onChange={(e) => setSortBy(e.target.value)} className="bg-surface-container-high text-sm rounded px-2 py-1">
            <option value="title">title</option>
            <option value="artist">artist</option>
            <option value="duration">duration</option>
            <option value="added_at">added_at</option>
          </select>
          <select value={sortOrder} onChange={(e) => setSortOrder(e.target.value)} className="bg-surface-container-high text-sm rounded px-2 py-1">
            <option value="asc">asc</option>
            <option value="desc">desc</option>
          </select>
          <button type="button" onClick={handleSort} className="text-xs uppercase tracking-wider bg-primary/20 text-primary px-3 py-1 rounded-full">apply</button>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-xs uppercase tracking-wider text-on-surface-variant">Filter</span>
          <input
            type="text"
            value={filterGenre}
            onChange={(e) => setFilterGenre(e.target.value)}
            placeholder="genre"
            className="bg-surface-container-high text-sm rounded px-2 py-1 w-32"
          />
          <button type="button" onClick={handleFilter} className="text-xs uppercase tracking-wider bg-primary/20 text-primary px-3 py-1 rounded-full">apply</button>
        </div>
        <div className="flex items-center gap-2 flex-wrap">
          <span className="text-xs uppercase tracking-wider text-on-surface-variant">Transform</span>
          <button type="button" onClick={() => handleTransform('reverse')} className="text-xs px-3 py-1 rounded-full bg-white/5 hover:bg-white/10">reverse</button>
          <button type="button" onClick={() => handleTransform('dedupe')} className="text-xs px-3 py-1 rounded-full bg-white/5 hover:bg-white/10">dedupe</button>
          <input
            type="number"
            value={transformN}
            min={0}
            onChange={(e) => setTransformN(Number(e.target.value))}
            className="bg-surface-container-high text-sm rounded px-2 py-1 w-16"
          />
          <button type="button" onClick={() => handleTransform('take', transformN)} className="text-xs px-3 py-1 rounded-full bg-white/5 hover:bg-white/10">take</button>
          <button type="button" onClick={() => handleTransform('drop', transformN)} className="text-xs px-3 py-1 rounded-full bg-white/5 hover:bg-white/10">drop</button>
        </div>
      </section>

      {derived && (
        <div className="flex items-center justify-between bg-surface-container-low border border-outline-variant/30 rounded-lg px-4 py-2 text-sm">
          <span>Derived view → <span className="text-primary">{derived.label}</span> ({derived.songs.length} songs). Source playlist unchanged.</span>
          <button type="button" onClick={() => setDerived(null)} className="text-xs uppercase tracking-wider text-on-surface-variant hover:text-white">show source</button>
        </div>
      )}

      {error && <div className="text-error text-sm">{error}</div>}

      <section className="flex flex-col gap-1">
        {visible.length === 0 ? (
          <div className="py-12 text-center text-on-surface-variant">Empty.</div>
        ) : (
          visible.map((s, idx) => (
            <div key={`${s.id}-${idx}`} className="group grid grid-cols-[2.5rem_1fr_10rem_4rem_3rem] gap-3 items-center px-3 py-2 rounded-md hover:bg-white/5">
              <button type="button" onClick={() => onPlay(s, visible, idx)} className="text-on-surface-variant group-hover:text-primary text-left">
                <span className="material-symbols-outlined hidden group-hover:inline fill text-[20px]">play_arrow</span>
                <span className="inline group-hover:hidden text-sm">{idx + 1}</span>
              </button>
              <div className="min-w-0">
                <div className="text-sm text-white truncate">{s.title}</div>
                <div className="text-xs text-on-surface-variant truncate md:hidden">{s.artist}</div>
              </div>
              <div className="hidden md:block text-sm text-on-surface-variant truncate">{s.artist}</div>
              <div className="text-sm text-on-surface-variant text-right">{formatDuration(s.duration_sec)}</div>
              {!derived && (
                <button
                  type="button"
                  onClick={() => removeSong(s.id)}
                  className="p-1 rounded-full hover:bg-white/10 text-on-surface-variant hover:text-error"
                  title="Remove from playlist"
                >
                  <span className="material-symbols-outlined text-[20px]">remove</span>
                </button>
              )}
            </div>
          ))
        )}
      </section>
    </div>
  );
};
