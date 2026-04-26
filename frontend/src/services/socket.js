// WebSocket client for the SpotiCry server protocol.
// Owns: connection lifecycle, request/response correlation by id, exponential
// reconnection, listener fan-out for unsolicited frames (STREAM_CHUNK,
// LIBRARY_UPDATED). All exports are pure factories — no module-level state.

const DEFAULT_REQ_TIMEOUT_MS = 5000;
const DEFAULT_RECONNECT_INITIAL_MS = 500;
const DEFAULT_RECONNECT_MAX_MS = 10_000;

const newId = () =>
  globalThis.crypto?.randomUUID?.() ??
  `r-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;

/**
 * Open a SpotiCry WebSocket connection.
 *
 * @param {string} url - e.g. `ws://localhost:7878/`
 * @param {object} [opts]
 * @param {number} [opts.requestTimeoutMs]
 * @param {number} [opts.reconnectInitialMs]
 * @param {number} [opts.reconnectMaxMs]
 * @param {(state: 'connecting'|'open'|'closed', detail?: object) => void} [opts.onStatus]
 * @returns {{
 *   send: (payload: object) => Promise<object>,
 *   onNotify: (listener: (msg: object) => void) => () => void,
 *   close: () => void,
 *   ready: () => Promise<void>,
 * }}
 */
export const connect = (url, opts = {}) => {
  const {
    requestTimeoutMs = DEFAULT_REQ_TIMEOUT_MS,
    reconnectInitialMs = DEFAULT_RECONNECT_INITIAL_MS,
    reconnectMaxMs = DEFAULT_RECONNECT_MAX_MS,
    onStatus = () => {},
  } = opts;

  // Pending requests by id → { resolve, reject, timer }.
  const pending = new Map();
  const notifyListeners = new Set();

  let ws = null;
  let closed = false;
  let backoff = reconnectInitialMs;
  let openWaiters = [];

  const dispatchNotify = (msg) => {
    notifyListeners.forEach((l) => {
      try {
        l(msg);
      } catch (e) {
        console.error('socket notify listener threw', e);
      }
    });
  };

  const handleResponse = (msg) => {
    if (msg.id && pending.has(msg.id)) {
      const entry = pending.get(msg.id);
      pending.delete(msg.id);
      clearTimeout(entry.timer);
      if (msg.status === 'ok') entry.resolve(msg);
      else entry.reject(Object.assign(new Error(msg.error?.message ?? 'request failed'), { code: msg.error?.code, response: msg }));
      return;
    }
    // Unsolicited frame (STREAM_CHUNK, LIBRARY_UPDATED, server-initiated).
    dispatchNotify(msg);
  };

  const flushOpenWaiters = (err) => {
    const waiters = openWaiters;
    openWaiters = [];
    waiters.forEach((w) => (err ? w.reject(err) : w.resolve()));
  };

  const rejectAllPending = (err) => {
    pending.forEach((entry) => {
      clearTimeout(entry.timer);
      entry.reject(err);
    });
    pending.clear();
  };

  const open = () => {
    onStatus('connecting', { url });
    const sock = new WebSocket(url);
    ws = sock;

    sock.addEventListener('open', () => {
      backoff = reconnectInitialMs;
      onStatus('open');
      flushOpenWaiters();
    });

    sock.addEventListener('message', (ev) => {
      let msg;
      try {
        msg = JSON.parse(ev.data);
      } catch (e) {
        console.error('socket: malformed JSON frame', ev.data);
        return;
      }
      handleResponse(msg);
    });

    sock.addEventListener('close', (ev) => {
      onStatus('closed', { code: ev.code, reason: ev.reason });
      if (closed) return;
      const err = Object.assign(new Error('socket closed'), { code: 'SOCKET_CLOSED' });
      rejectAllPending(err);
      flushOpenWaiters(err);
      const delay = Math.min(backoff, reconnectMaxMs);
      backoff = Math.min(backoff * 2, reconnectMaxMs);
      setTimeout(() => {
        if (!closed) open();
      }, delay);
    });

    sock.addEventListener('error', () => {
      // 'close' will fire next; reconnection handled there.
    });
  };

  open();

  const send = (payload) => {
    if (closed) return Promise.reject(new Error('socket closed by caller'));
    return new Promise((resolve, reject) => {
      const id = newId();
      const frame = JSON.stringify({ id, ...payload });
      const dispatch = () => {
        const timer = setTimeout(() => {
          pending.delete(id);
          reject(Object.assign(new Error(`request ${id} timed out`), { code: 'TIMEOUT' }));
        }, requestTimeoutMs);
        pending.set(id, { resolve, reject, timer });
        ws.send(frame);
      };
      if (ws && ws.readyState === WebSocket.OPEN) dispatch();
      else openWaiters.push({ resolve: dispatch, reject });
    });
  };

  const onNotify = (listener) => {
    notifyListeners.add(listener);
    return () => notifyListeners.delete(listener);
  };

  const ready = () =>
    new Promise((resolve, reject) => {
      if (ws && ws.readyState === WebSocket.OPEN) resolve();
      else openWaiters.push({ resolve, reject });
    });

  const close = () => {
    closed = true;
    rejectAllPending(new Error('socket closed by caller'));
    if (ws) ws.close();
  };

  return { send, onNotify, close, ready };
};
