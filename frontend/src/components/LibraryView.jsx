// Library view — the main song listing. Three search criteria per spec §3:
//   1. text contains over title or artist  (substring match — client-side)
//   2. genre exact match                   (string equality — client-side)
//   3. duration range (min/max in mm:ss)   (numeric range — SERVER-side via
//                                           LIST_SONGS { criteria: range })
// Mixing local + server keeps text/genre snappy while exercising a genuinely
// different server resolution path for duration (range comparator vs the
// substring/equality paths).

import React, { useEffect, useMemo, useState } from 'react';

import { listSongs, durationBetween } from '../services/api.js';

const formatDuration = (sec) => {
  const s = Math.max(0, Math.floor(sec || 0));
  const m = Math.floor(s / 60);
  const r = s % 60;
  return `${m}:${r.toString().padStart(2, '0')}`;
};

const matchesTextField = (song, field, needle) =>
  needle.length === 0 ||
  ((field === 'title' ? song.title : song.artist) ?? '').toLowerCase().includes(needle.toLowerCase());

// "1:30" → 90, "90" → 90, "" → null. Anything unparseable → null (= no bound).
const parseDuration = (raw) => {
  const s = (raw ?? '').trim();
  if (s === '') return null;
  if (s.includes(':')) {
    const [m, r] = s.split(':');
    const mn = Number(m);
    const sn = Number(r);
    if (!Number.isFinite(mn) || !Number.isFinite(sn)) return null;
    return mn * 60 + sn;
  }
  const n = Number(s);
  return Number.isFinite(n) ? n : null;
};

const inDurationRange = (val, min, max) =>
  (min == null || val >= min) && (max == null || val <= max);

export const LibraryView = ({ socket, songs, playlists, onPlay, onAddToPlaylist }) => {
  const [textField, setTextField] = useState('title');
  const [text, setText] = useState('');
  const [genre, setGenre] = useState('');
  const [durMin, setDurMin] = useState('');
  const [durMax, setDurMax] = useState('');
  const [serverFiltered, setServerFiltered] = useState(null);
  const [serverFilterStatus, setServerFilterStatus] = useState('idle');

  const genres = useMemo(() => {
    const set = new Set(songs.map((s) => s.genre).filter(Boolean));
    return Array.from(set).sort();
  }, [songs]);

  // Duration filter is resolved server-side: when at least one bound is set,
  // we issue a LIST_SONGS with a `range` criterion (debounced 250ms). The
  // server applies the numeric comparator; we then layer text/genre on the
  // returned set client-side. When both bounds clear, we drop back to the
  // unfiltered `songs` prop.
  useEffect(() => {
    const min = parseDuration(durMin);
    const max = parseDuration(durMax);
    if (min == null && max == null) {
      setServerFiltered(null);
      setServerFilterStatus('idle');
      return undefined;
    }
    if (!socket) return undefined;
    setServerFilterStatus('loading');
    let cancelled = false;
    const timer = setTimeout(async () => {
      try {
        const data = await listSongs(
          socket,
          durationBetween(min ?? 0, max ?? Number.MAX_SAFE_INTEGER),
        );
        if (cancelled) return;
        setServerFiltered(data.songs ?? []);
        setServerFilterStatus('ok');
      } catch (e) {
        if (cancelled) return;
        console.warn('duration server search failed', e);
        setServerFilterStatus('error');
      }
    }, 250);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
    // `songs` in deps so a CLI add/remove (LIBRARY_UPDATED notification) also
    // re-runs the server query — otherwise the filtered set would go stale
    // until the user retypes a bound.
  }, [socket, durMin, durMax, songs]);

  const baseSet = serverFiltered ?? songs;

  const filtered = useMemo(() => {
    return baseSet
      .filter((s) => matchesTextField(s, textField, text.trim()))
      .filter((s) => genre === '' || s.genre === genre);
  }, [baseSet, textField, text, genre]);

  return (
    <div className="p-gutter lg:p-10 flex flex-col gap-8">
      <header className="flex flex-col gap-2">
        <span className="text-[#8B5CF6] text-xs uppercase tracking-widest font-semibold">Library</span>
        <h1 className="text-display-lg drop-shadow-lg">{songs.length} song{songs.length === 1 ? '' : 's'}</h1>
      </header>

      <section className="grid gap-4 grid-cols-1 md:grid-cols-4 bg-surface-container/60 backdrop-blur rounded-xl p-4 border border-white/5">
        <div className="flex items-center gap-3 col-span-1 md:col-span-2 bg-surface-container-high rounded-full px-4 py-2 border border-outline-variant/30 transition-colors focus-within:border-white/30 focus-within:bg-white/5">
          <div className="flex items-center">
            <select
              value={textField}
              onChange={(e) => setTextField(e.target.value)}
              className="bg-transparent text-sm font-semibold text-[#8B5CF6] border-none focus:ring-0 cursor-pointer outline-none pr-8"
            >
              <option value="title" className="bg-[#121212] text-white">Title</option>
              <option value="artist" className="bg-[#121212] text-white">Artist</option>
            </select>
          </div>
          <div className="w-[1px] h-4 bg-white/10 mx-2"></div>
          <span className="material-symbols-outlined text-outline text-[18px]">search</span>
          <input
            type="text"
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder="contains…"
            className="bg-transparent border-none focus:ring-0 focus:outline-none text-white w-full placeholder-white/30 text-sm"
          />
        </div>
        <div className="flex items-center bg-surface-container-high rounded-full px-4 py-2 border border-outline-variant/30 transition-colors focus-within:border-white/30 focus-within:bg-white/5">
          <select
            value={genre}
            onChange={(e) => setGenre(e.target.value)}
            className="bg-transparent w-full text-sm font-medium text-white border-none focus:ring-0 cursor-pointer outline-none pr-6"
          >
            <option value="" className="bg-[#121212] text-white">All genres</option>
            {genres.map((g) => (
              <option key={g} value={g} className="bg-[#121212] text-white">{g}</option>
            ))}
          </select>
        </div>
        <div
          className="flex items-center gap-2 bg-surface-container-high rounded-full px-4 py-2 border border-outline-variant/30 transition-colors focus-within:border-white/30 focus-within:bg-white/5"
          title="Duration filter is resolved server-side via LIST_SONGS range criteria"
        >
          <span
            className={`material-symbols-outlined text-[18px] ${
              serverFilterStatus === 'loading'
                ? 'text-amber-400 animate-pulse'
                : serverFilterStatus === 'error'
                  ? 'text-error'
                  : serverFilterStatus === 'ok'
                    ? 'text-primary'
                    : 'text-outline'
            }`}
          >
            {serverFilterStatus === 'error' ? 'error' : 'schedule'}
          </span>
          <input
            type="text"
            value={durMin}
            onChange={(e) => setDurMin(e.target.value)}
            placeholder="min"
            aria-label="Minimum duration (mm:ss or seconds)"
            className="bg-transparent border-none focus:ring-0 focus:outline-none text-white w-full placeholder-white/30 text-sm min-w-0"
          />
          <span className="text-white/30 text-xs">–</span>
          <input
            type="text"
            value={durMax}
            onChange={(e) => setDurMax(e.target.value)}
            placeholder="max"
            aria-label="Maximum duration (mm:ss or seconds)"
            className="bg-transparent border-none focus:ring-0 focus:outline-none text-white w-full placeholder-white/30 text-sm min-w-0"
          />
          {(durMin || durMax) && (
            <button
              type="button"
              onClick={() => { setDurMin(''); setDurMax(''); }}
              className="text-on-surface-variant hover:text-white"
              title="Clear duration filter"
            >
              <span className="material-symbols-outlined text-[16px]">close</span>
            </button>
          )}
        </div>
      </section>

      <section className="flex flex-col gap-1">
        <div className="grid grid-cols-[2.5rem_1fr_10rem_8rem_4rem_3rem] gap-3 px-3 py-2 text-[11px] uppercase tracking-widest text-on-surface-variant border-b border-white/5">
          <span>#</span>
          <span>Title</span>
          <span className="hidden md:block">Artist</span>
          <span className="hidden md:block">Genre</span>
          <span className="text-right">Time</span>
          <span></span>
        </div>
        {filtered.length === 0 ? (
          <div className="py-12 text-center text-on-surface-variant">No songs match the current filters.</div>
        ) : (
          filtered.map((song, idx) => (
            <SongRow
              key={song.id}
              index={idx + 1}
              song={song}
              playlists={playlists}
              onPlay={() => onPlay(song, filtered, idx)}
              onAddToPlaylist={(playlistId) => onAddToPlaylist(playlistId, song.id)}
              formatDuration={formatDuration}
            />
          ))
        )}
      </section>
    </div>
  );
};

const SongRow = ({ index, song, playlists, onPlay, onAddToPlaylist, formatDuration }) => {
  const [menu, setMenu] = useState(false);
  return (
    <div className="group grid grid-cols-[2.5rem_1fr_10rem_8rem_4rem_3rem] gap-3 items-center px-3 py-2 rounded-md hover:bg-white/5 transition-colors duration-150 relative">
      <button
        type="button"
        onClick={onPlay}
        className="text-on-surface-variant group-hover:text-primary group-hover:fill text-left"
        title="Play"
      >
        <span className="material-symbols-outlined hidden group-hover:inline fill text-[20px]">play_arrow</span>
        <span className="inline group-hover:hidden text-sm">{index}</span>
      </button>
      <div className="min-w-0">
        <div className="text-sm text-white truncate">{song.title}</div>
        <div className="text-xs text-on-surface-variant truncate md:hidden">{song.artist}</div>
      </div>
      <div className="hidden md:block text-sm text-on-surface-variant truncate">{song.artist}</div>
      <div className="hidden md:block text-sm text-on-surface-variant truncate">{song.genre}</div>
      <div className="text-sm text-on-surface-variant text-right">{formatDuration(song.duration_sec)}</div>
      <div className="relative">
        <button
          type="button"
          onClick={() => setMenu((v) => !v)}
          className="p-1 rounded-full hover:bg-white/10"
          title="Add to playlist"
        >
          <span className="material-symbols-outlined text-[20px]">playlist_add</span>
        </button>
        {menu && (
          <div className="absolute right-0 top-full mt-1 z-20 bg-surface-container-high border border-outline-variant rounded-lg shadow-2xl min-w-[200px] py-2">
            {playlists.length === 0 ? (
              <div className="px-3 py-2 text-xs text-on-surface-variant">No playlists yet</div>
            ) : (
              playlists.map((p) => (
                <button
                  key={p.id}
                  type="button"
                  onClick={() => {
                    onAddToPlaylist(p.id);
                    setMenu(false);
                  }}
                  className="w-full text-left px-3 py-2 text-sm hover:bg-white/10"
                >
                  {p.name}
                </button>
              ))
            )}
          </div>
        )}
      </div>
    </div>
  );
};
