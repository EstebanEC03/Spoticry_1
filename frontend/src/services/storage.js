// localStorage wrapper for client-side playlist persistence with sync against
// the server. Decision §7: client mantiene playlists locales, sincroniza al
// arrancar. Conflict resolution: highest `version` wins per playlist id.
//
// All exports are pure functions — no module-level state. The caller wires
// these to a connected socket from `socket.js`.

import { listPlaylists } from './api.js';

const KEY_PLAYLISTS = 'spoticry.playlists.v1';
const KEY_CLIENT_ID = 'spoticry.client_id.v1';

const safeParse = (raw, fallback) => {
  if (!raw) return fallback;
  try {
    return JSON.parse(raw);
  } catch {
    return fallback;
  }
};

export const loadLocalPlaylists = () =>
  safeParse(globalThis.localStorage?.getItem(KEY_PLAYLISTS), []);

export const saveLocalPlaylists = (playlists) => {
  if (!globalThis.localStorage) return;
  globalThis.localStorage.setItem(KEY_PLAYLISTS, JSON.stringify(playlists));
};

export const loadClientId = () => {
  const ls = globalThis.localStorage;
  if (!ls) return crypto.randomUUID();
  const existing = ls.getItem(KEY_CLIENT_ID);
  if (existing) return existing;
  const fresh = crypto.randomUUID();
  ls.setItem(KEY_CLIENT_ID, fresh);
  return fresh;
};

// Merge two playlist lists by id; highest version wins. Pure function — no
// mutation on either input.
export const mergePlaylists = (a, b) => {
  const indexed = new Map();
  [...a, ...b].forEach((p) => {
    const prev = indexed.get(p.id);
    if (!prev || (p.version ?? 1) >= (prev.version ?? 1)) indexed.set(p.id, p);
  });
  return Array.from(indexed.values());
};

/**
 * Sync local playlists against server. Fetches server list, merges by version,
 * persists the merged result locally, and returns it. Read-only against the
 * server (does not push local-only playlists; that's a follow-up).
 */
export const syncPlaylists = async (socket, owner) => {
  const local = loadLocalPlaylists();
  let remote = [];
  try {
    const data = await listPlaylists(socket, owner);
    remote = data.playlists ?? [];
  } catch (e) {
    console.warn('syncPlaylists: server fetch failed, using local only', e);
    return local;
  }
  const merged = mergePlaylists(local, remote);
  saveLocalPlaylists(merged);
  return merged;
};
