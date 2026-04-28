// Audio player hook. Owns a single <audio> element and a streaming pipeline
// that picks one of two backends per playback:
//
//   - MSE backend: MediaSource + SourceBuffer fed by STREAM_CHUNK frames.
//     Allows seeking inside the already-buffered range while bytes are still
//     arriving (the spec's "buffer local de la canción actual" requirement).
//     Used when the browser actually supports MSE for `audio/mpeg`
//     (Chrome/Edge on Windows, Linux, macOS).
//
//   - Blob backend: chunks accumulate in memory, then on EOF we hand the
//     <audio> element a single Blob URL. No progressive seek, but works on
//     every browser that can decode MP3 — Firefox and Safari, in particular,
//     do not implement MSE for `audio/mpeg`, and some Windows builds expose
//     MediaSource but fail at addSourceBuffer time.
//
// Detection runs at hook init and again per-play with a try/catch fence: if
// MSE setup throws we tear it down and fall through to the Blob backend
// without dropping the stream.
//
// Server emits STREAM_CHUNK frames over WS regardless of backend; the hook
// only changes how it consumes them.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { playSong, stopSong } from '../services/api.js';

const MIME = 'audio/mpeg';

const detectMseSupport = () => {
  if (typeof window === 'undefined') return false;
  const MS = window.MediaSource || window.WebKitMediaSource;
  if (!MS || typeof MS.isTypeSupported !== 'function') return false;
  try {
    return MS.isTypeSupported(MIME);
  } catch {
    return false;
  }
};

const base64ToBytes = (b64) => {
  const bin = atob(b64);
  return Uint8Array.from(bin, (c) => c.charCodeAt(0));
};

const concatChunks = (chunks) => {
  const total = chunks.reduce((acc, c) => acc + c.length, 0);
  const merged = new Uint8Array(total);
  let offset = 0;
  for (const c of chunks) {
    merged.set(c, offset);
    offset += c.length;
  }
  return merged;
};

export const usePlayer = (socket) => {
  const audioRef = useRef(null);
  const mediaSourceRef = useRef(null);
  const sourceBufferRef = useRef(null);
  const queueRef = useRef([]);
  const streamIdRef = useRef(null);
  const eofRef = useRef(false);
  // 'mse' = progressive append into SourceBuffer.
  // 'blob' = accumulate bytes, swap audio.src on EOF.
  const modeRef = useRef('mse');
  const blobUrlRef = useRef(null);

  const [currentSong, setCurrentSong] = useState(null);
  const [isPlaying, setIsPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolumeState] = useState(0.8);
  const [bufferedSeconds, setBufferedSeconds] = useState(0);
  const [queue, setQueue] = useState([]);
  const [queueIndex, setQueueIndex] = useState(-1);

  useEffect(() => {
    const audio = new Audio();
    audio.preload = 'auto';
    audio.crossOrigin = 'anonymous';
    audio.volume = volume;
    audioRef.current = audio;

    const onTime = () => setCurrentTime(audio.currentTime);
    const onDur = () => setDuration(Number.isFinite(audio.duration) ? audio.duration : 0);
    const onPlay = () => setIsPlaying(true);
    const onPause = () => setIsPlaying(false);
    const onProgress = () => {
      if (audio.buffered.length > 0) {
        setBufferedSeconds(audio.buffered.end(audio.buffered.length - 1));
      }
    };

    audio.addEventListener('timeupdate', onTime);
    audio.addEventListener('durationchange', onDur);
    audio.addEventListener('play', onPlay);
    audio.addEventListener('pause', onPause);
    audio.addEventListener('progress', onProgress);

    return () => {
      audio.removeEventListener('timeupdate', onTime);
      audio.removeEventListener('durationchange', onDur);
      audio.removeEventListener('play', onPlay);
      audio.removeEventListener('pause', onPause);
      audio.removeEventListener('progress', onProgress);
      audio.pause();
    };
  }, []);

  const flushMseQueue = useCallback(() => {
    const sb = sourceBufferRef.current;
    const ms = mediaSourceRef.current;
    if (!sb || sb.updating || !ms || ms.readyState !== 'open') return;
    if (queueRef.current.length === 0) {
      if (eofRef.current) {
        try {
          ms.endOfStream();
        } catch {
          /* already ended */
        }
      }
      return;
    }
    const next = queueRef.current.shift();
    try {
      sb.appendBuffer(next);
    } catch (e) {
      console.error('appendBuffer failed', e);
    }
  }, []);

  const finalizeBlob = useCallback(() => {
    const audio = audioRef.current;
    if (!audio) return;
    const merged = concatChunks(queueRef.current);
    queueRef.current = [];
    const blob = new Blob([merged], { type: MIME });
    if (blobUrlRef.current) {
      try { URL.revokeObjectURL(blobUrlRef.current); } catch { /* ignore */ }
    }
    const url = URL.createObjectURL(blob);
    blobUrlRef.current = url;
    audio.src = url;
    audio.load();
    audio.play().catch(() => {});
  }, []);

  // STREAM_CHUNK listener — branches on the active backend.
  useEffect(() => {
    if (!socket) return undefined;
    return socket.onNotify((msg) => {
      if (msg.type !== 'STREAM_CHUNK') return;
      if (msg.stream_id !== streamIdRef.current) return;

      if (modeRef.current === 'mse') {
        if (msg.eof) {
          eofRef.current = true;
          flushMseQueue();
          return;
        }
        const bytes = base64ToBytes(msg.payload_b64);
        queueRef.current.push(bytes);
        flushMseQueue();
        return;
      }

      // Blob backend: collect everything, finalize on EOF.
      if (msg.eof) {
        eofRef.current = true;
        finalizeBlob();
        return;
      }
      queueRef.current.push(base64ToBytes(msg.payload_b64));
    });
  }, [socket, flushMseQueue, finalizeBlob]);

  const tearDown = useCallback(() => {
    queueRef.current = [];
    eofRef.current = false;
    streamIdRef.current = null;
    if (sourceBufferRef.current) {
      try {
        const ms = mediaSourceRef.current;
        if (ms && ms.readyState === 'open') {
          ms.removeSourceBuffer(sourceBufferRef.current);
        }
      } catch {
        /* ignore */
      }
      sourceBufferRef.current = null;
    }
    mediaSourceRef.current = null;
    if (blobUrlRef.current) {
      try { URL.revokeObjectURL(blobUrlRef.current); } catch { /* ignore */ }
      blobUrlRef.current = null;
    }
    if (audioRef.current) {
      try {
        if (audioRef.current.src && audioRef.current.src.startsWith('blob:')) {
          URL.revokeObjectURL(audioRef.current.src);
        }
      } catch {
        /* ignore */
      }
      audioRef.current.removeAttribute('src');
      audioRef.current.load();
    }
  }, []);

  // Tries to install the MSE pipeline. Returns true on success, false if
  // setup throws (browser exposes MediaSource but rejects audio/mpeg, etc).
  const setupMse = useCallback(async () => {
    if (!detectMseSupport()) return false;
    try {
      const ms = new MediaSource();
      mediaSourceRef.current = ms;
      audioRef.current.src = URL.createObjectURL(ms);
      await new Promise((resolve, reject) => {
        const onOpen = () => { ms.removeEventListener('error', onErr); resolve(); };
        const onErr = (e) => { ms.removeEventListener('sourceopen', onOpen); reject(e); };
        ms.addEventListener('sourceopen', onOpen, { once: true });
        ms.addEventListener('error', onErr, { once: true });
      });
      const sb = ms.addSourceBuffer(MIME);
      sb.addEventListener('updateend', flushMseQueue);
      sourceBufferRef.current = sb;
      return true;
    } catch (e) {
      console.warn('MSE setup failed, falling back to blob backend:', e);
      try { mediaSourceRef.current = null; } catch { /* ignore */ }
      if (audioRef.current?.src?.startsWith('blob:')) {
        try { URL.revokeObjectURL(audioRef.current.src); } catch { /* ignore */ }
        audioRef.current.removeAttribute('src');
      }
      sourceBufferRef.current = null;
      return false;
    }
  }, [flushMseQueue]);

  const play = useCallback(
    async (song, contextQueue = null, contextIndex = -1) => {
      if (!socket || !audioRef.current) return;
      const previous = streamIdRef.current;
      tearDown();
      if (previous) {
        stopSong(socket, previous).catch(() => {});
      }

      const mseOk = await setupMse();
      modeRef.current = mseOk ? 'mse' : 'blob';

      try {
        const ack = await playSong(socket, song.id);
        streamIdRef.current = ack.stream_id;
        setCurrentSong(song);
        setDuration(Number(ack.duration_sec) || 0);
        if (Array.isArray(contextQueue) && contextQueue.length > 0) {
          const idx = contextIndex >= 0
            ? contextIndex
            : contextQueue.findIndex((s) => s.id === song.id);
          setQueue(contextQueue);
          setQueueIndex(idx);
        } else {
          setQueue([]);
          setQueueIndex(-1);
        }
        if (mseOk) {
          await audioRef.current.play().catch(() => {});
        }
        // Blob backend: playback starts inside finalizeBlob() once EOF arrives.
      } catch (e) {
        console.error('play failed', e);
        tearDown();
      }
    },
    [socket, tearDown, setupMse]
  );

  const hasPrev = queueIndex > 0 && queue.length > 0;
  const hasNext = queueIndex >= 0 && queueIndex < queue.length - 1;

  const playPrev = useCallback(() => {
    if (!hasPrev) return;
    const idx = queueIndex - 1;
    play(queue[idx], queue, idx);
  }, [hasPrev, queue, queueIndex, play]);

  const playNext = useCallback(() => {
    if (!hasNext) return;
    const idx = queueIndex + 1;
    play(queue[idx], queue, idx);
  }, [hasNext, queue, queueIndex, play]);

  const pause = useCallback(() => {
    audioRef.current?.pause();
  }, []);

  const resume = useCallback(() => {
    audioRef.current?.play().catch(() => {});
  }, []);

  const stop = useCallback(async () => {
    const sid = streamIdRef.current;
    tearDown();
    setCurrentSong(null);
    setIsPlaying(false);
    setCurrentTime(0);
    setBufferedSeconds(0);
    setQueue([]);
    setQueueIndex(-1);
    if (socket && sid) {
      stopSong(socket, sid).catch(() => {});
    }
  }, [socket, tearDown]);

  const seek = useCallback((seconds) => {
    const audio = audioRef.current;
    if (!audio) return;
    // Blob backend has the whole file once playback starts, so allow seeking
    // up to audio.duration. MSE backend caps at the buffered tail.
    const ceiling = modeRef.current === 'blob'
      ? (audio.duration || 0)
      : (bufferedSeconds || audio.duration || 0);
    const clamped = Math.max(0, Math.min(seconds, ceiling));
    audio.currentTime = clamped;
  }, [bufferedSeconds]);

  const setVolume = useCallback((v) => {
    const clamped = Math.max(0, Math.min(1, v));
    setVolumeState(clamped);
    if (audioRef.current) audioRef.current.volume = clamped;
  }, []);

  return useMemo(
    () => ({
      currentSong,
      isPlaying,
      currentTime,
      duration,
      bufferedSeconds,
      volume,
      play,
      pause,
      resume,
      stop,
      seek,
      setVolume,
      playPrev,
      playNext,
      hasPrev,
      hasNext,
    }),
    [currentSong, isPlaying, currentTime, duration, bufferedSeconds, volume, play, pause, resume, stop, seek, setVolume, playPrev, playNext, hasPrev, hasNext]
  );
};
