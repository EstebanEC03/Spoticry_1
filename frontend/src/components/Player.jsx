import React from 'react';

const formatTime = (sec) => {
  const s = Math.max(0, Math.floor(sec || 0));
  const m = Math.floor(s / 60);
  const r = s % 60;
  return `${m}:${r.toString().padStart(2, '0')}`;
};

export const Player = ({ player }) => {
  const { currentSong, isPlaying, currentTime, duration, bufferedSeconds, volume, pause, resume, stop, seek, setVolume, playPrev, playNext, hasPrev, hasNext } = player;
  const total = duration || currentSong?.duration_sec || 0;

  return (
    <div className="hidden md:flex bg-[#0e0e0e]/95 backdrop-blur-2xl bottom-0 h-[96px] border-t border-white/10 shadow-[0_-10px_40px_rgba(0,0,0,0.5)] fixed left-0 w-full z-50 justify-between items-center px-8">
      <div className="flex items-center gap-4 w-1/3 min-w-0">
        <div className="w-14 h-14 rounded shadow-md bg-gradient-to-br from-violet-500/40 via-purple-600/30 to-blue-500/30 flex items-center justify-center">
          <span className="material-symbols-outlined text-white/40">music_note</span>
        </div>
        <div className="flex flex-col min-w-0">
          <span className="text-white text-sm truncate">{currentSong?.title ?? '—'}</span>
          <span className="text-on-surface-variant text-xs truncate">{currentSong?.artist ?? 'No song playing'}</span>
        </div>
      </div>

      <div className="flex flex-col items-center justify-center gap-2 w-1/3">
        <div className="flex items-center gap-6">
          <button
            type="button"
            onClick={playPrev}
            disabled={!hasPrev}
            className="text-neutral-400 hover:text-white disabled:opacity-30 disabled:cursor-not-allowed transition-transform"
            title="Previous song"
          >
            <span className="material-symbols-outlined">skip_previous</span>
          </button>
          <button
            type="button"
            onClick={() => seek(Math.max(0, currentTime - 10))}
            disabled={!currentSong}
            className="text-neutral-400 hover:text-white disabled:opacity-30 disabled:cursor-not-allowed transition-transform"
            title="Back 10s"
          >
            <span className="material-symbols-outlined">replay_10</span>
          </button>
          {isPlaying ? (
            <button
              type="button"
              onClick={pause}
              disabled={!currentSong}
              className="text-violet-400 scale-110 flex items-center justify-center hover:scale-125 transition-transform bg-white/5 rounded-full p-1 disabled:opacity-30"
              title="Pause"
            >
              <span className="material-symbols-outlined fill text-[36px]">pause_circle</span>
            </button>
          ) : (
            <button
              type="button"
              onClick={() => (currentSong ? resume() : null)}
              disabled={!currentSong}
              className="text-violet-400 scale-110 flex items-center justify-center hover:scale-125 transition-transform bg-white/5 rounded-full p-1 disabled:opacity-30"
              title="Play"
            >
              <span className="material-symbols-outlined fill text-[36px]">play_circle</span>
            </button>
          )}
          <button
            type="button"
            onClick={() => seek(Math.min(bufferedSeconds || total, currentTime + 10))}
            disabled={!currentSong}
            className="text-neutral-400 hover:text-white disabled:opacity-30 disabled:cursor-not-allowed transition-transform"
            title="Forward 10s"
          >
            <span className="material-symbols-outlined">forward_10</span>
          </button>
          <button
            type="button"
            onClick={playNext}
            disabled={!hasNext}
            className="text-neutral-400 hover:text-white disabled:opacity-30 disabled:cursor-not-allowed transition-transform"
            title="Next song"
          >
            <span className="material-symbols-outlined">skip_next</span>
          </button>
          <button
            type="button"
            onClick={stop}
            disabled={!currentSong}
            className="text-neutral-400 hover:text-white disabled:opacity-30 disabled:cursor-not-allowed transition-transform"
            title="Stop"
          >
            <span className="material-symbols-outlined">stop_circle</span>
          </button>
        </div>
        <div className="w-full max-w-md flex items-center gap-2 text-xs text-on-surface-variant">
          <span className="w-10 text-right">{formatTime(currentTime)}</span>
          <div className="h-1 flex-1 bg-surface-bright rounded-full overflow-hidden relative">
            {/* Buffered range */}
            <div
              className="absolute inset-y-0 left-0 bg-white/15"
              style={{ width: total ? `${Math.min(100, (bufferedSeconds / total) * 100)}%` : '0%' }}
            />
            {/* Played range */}
            <div
              className="absolute inset-y-0 left-0 bg-primary"
              style={{ width: total ? `${Math.min(100, (currentTime / total) * 100)}%` : '0%' }}
            />
            <input
              type="range"
              min={0}
              max={total || 0}
              step={0.1}
              value={currentTime}
              onChange={(e) => seek(Number(e.target.value))}
              className="absolute inset-0 w-full opacity-0 cursor-pointer"
            />
          </div>
          <span className="w-10">{formatTime(total)}</span>
        </div>
      </div>

      <div className="flex items-center justify-end gap-3 w-1/3">
        <span className="material-symbols-outlined text-neutral-400 text-[20px]">volume_up</span>
        <input
          type="range"
          min={0}
          max={1}
          step={0.01}
          value={volume}
          onChange={(e) => setVolume(Number(e.target.value))}
          className="w-32"
        />
      </div>
    </div>
  );
};
