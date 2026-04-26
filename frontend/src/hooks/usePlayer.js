// Audio player hook. Owns a single <audio> element + MediaSource pipeline.
// Server emits STREAM_CHUNK frames over WS; this hook decodes them and
// appends bytes to a SourceBuffer. The audio element seeks against the
// already-buffered range, so adelantar/retroceder over what arrived works
// without any extra round-trip — exactly the spec's "buffer local de la
// canción actual" requirement.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { playSong, stopSong } from '../services/api.js';

const MIME = 'audio/mpeg';

const base64ToBytes = (b64) => {
  const bin = atob(b64);
  // Functional: build Uint8Array via Array.from + map (no for-loop mutation).
  return Uint8Array.from(bin, (c) => c.charCodeAt(0));
};

export const usePlayer = (socket) => {
  const audioRef = useRef(null);
  const mediaSourceRef = useRef(null);
  const sourceBufferRef = useRef(null);
  const queueRef = useRef([]);
  const streamIdRef = useRef(null);
  const eofRef = useRef(false);

  const [currentSong, setCurrentSong] = useState(null);
  const [isPlaying, setIsPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolumeState] = useState(0.8);
  const [bufferedSeconds, setBufferedSeconds] = useState(0);
  const [queue, setQueue] = useState([]);
  const [queueIndex, setQueueIndex] = useState(-1);

  // One-time audio element initialization.
  useEffect(() => {
    const audio = new Audio();
    audio.preload = 'auto';
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

  const flushQueue = useCallback(() => {
    const sb = sourceBufferRef.current;
    const ms = mediaSourceRef.current;
    if (!sb || sb.updating || !ms || ms.readyState !== 'open') return;
    if (queueRef.current.length === 0) {
      if (eofRef.current) {
        try {
          ms.endOfStream();
        } catch {
          /* ignore — already ended */
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

  // STREAM_CHUNK listener.
  useEffect(() => {
    if (!socket) return undefined;
    return socket.onNotify((msg) => {
      if (msg.type !== 'STREAM_CHUNK') return;
      if (msg.stream_id !== streamIdRef.current) return;
      if (msg.eof) {
        eofRef.current = true;
        flushQueue();
        return;
      }
      const bytes = base64ToBytes(msg.payload_b64);
      queueRef.current.push(bytes);
      flushQueue();
    });
  }, [socket, flushQueue]);

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
    if (audioRef.current) {
      try {
        URL.revokeObjectURL(audioRef.current.src);
      } catch {
        /* ignore */
      }
      audioRef.current.removeAttribute('src');
      audioRef.current.load();
    }
  }, []);

  const play = useCallback(
    async (song, contextQueue = null, contextIndex = -1) => {
      if (!socket || !audioRef.current) return;
      // Stop any previous stream first.
      const previous = streamIdRef.current;
      tearDown();
      if (previous) {
        stopSong(socket, previous).catch(() => {});
      }

      const ms = new MediaSource();
      mediaSourceRef.current = ms;
      audioRef.current.src = URL.createObjectURL(ms);

      await new Promise((resolve) => {
        ms.addEventListener('sourceopen', resolve, { once: true });
      });

      const sb = ms.addSourceBuffer(MIME);
      sb.addEventListener('updateend', flushQueue);
      sourceBufferRef.current = sb;

      try {
        const ack = await playSong(socket, song.id);
        streamIdRef.current = ack.stream_id;
        setCurrentSong(song);
        // Hint duration so the seek bar has a scale before durationchange fires.
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
        await audioRef.current.play().catch(() => {});
      } catch (e) {
        console.error('play failed', e);
        tearDown();
      }
    },
    [socket, flushQueue, tearDown]
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
    const clamped = Math.max(0, Math.min(seconds, bufferedSeconds || audio.duration || 0));
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
