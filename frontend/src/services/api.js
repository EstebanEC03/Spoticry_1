// Pure protocol functions over a connected socket. Each call serializes a
// request payload, awaits the correlated response, and returns the `data`
// field on success or throws an Error tagged with the protocol code on
// failure. Mirrors the contract in SERVER_API.md.

const call = async (socket, payload) => {
  const resp = await socket.send(payload);
  return resp.data;
};

// --- Handshake ------------------------------------------------------------

export const hello = (socket, clientId, version = '1.0') =>
  call(socket, { type: 'HELLO', client_id: clientId, version });

export const ping = (socket, ts = Date.now()) =>
  call(socket, { type: 'PING', ts });

// --- Songs ----------------------------------------------------------------

export const listSongs = (socket, criteria) =>
  call(socket, criteria ? { type: 'LIST_SONGS', criteria } : { type: 'LIST_SONGS' });

export const getSong = (socket, songId) =>
  call(socket, { type: 'GET_SONG', song_id: songId });

export const playSong = (socket, songId) =>
  call(socket, { type: 'PLAY_SONG', song_id: songId });

export const stopSong = (socket, streamId) =>
  call(socket, { type: 'STOP_SONG', stream_id: streamId });

// --- Playlists ------------------------------------------------------------

export const createPlaylist = (socket, name, owner) =>
  call(socket, { type: 'CREATE_PLAYLIST', name, owner });

export const deletePlaylist = (socket, playlistId) =>
  call(socket, { type: 'DELETE_PLAYLIST', playlist_id: playlistId });

export const listPlaylists = (socket, owner) =>
  call(socket, owner ? { type: 'LIST_PLAYLISTS', owner } : { type: 'LIST_PLAYLISTS' });

export const getPlaylist = (socket, playlistId) =>
  call(socket, { type: 'GET_PLAYLIST', playlist_id: playlistId });

export const addSongToPlaylist = (socket, playlistId, songId) =>
  call(socket, { type: 'ADD_SONG_TO_PLAYLIST', playlist_id: playlistId, song_id: songId });

export const removeSongFromPlaylist = (socket, playlistId, songId) =>
  call(socket, {
    type: 'REMOVE_SONG_FROM_PLAYLIST',
    playlist_id: playlistId,
    song_id: songId,
  });

export const filterPlaylist = (socket, playlistId, criteria) =>
  call(socket, { type: 'FILTER_PLAYLIST', playlist_id: playlistId, criteria });

export const sortPlaylist = (socket, playlistId, by, order = 'asc') =>
  call(socket, { type: 'SORT_PLAYLIST', playlist_id: playlistId, by, order });

export const transformPlaylist = (socket, playlistId, op, n) =>
  call(socket, n != null
    ? { type: 'TRANSFORM_PLAYLIST', playlist_id: playlistId, op, n }
    : { type: 'TRANSFORM_PLAYLIST', playlist_id: playlistId, op });

// --- Search criteria builders --------------------------------------------

export const titleContains = (value) => ({ field: 'title', op: 'contains', value });
export const artistContains = (value) => ({ field: 'artist', op: 'contains', value });
export const genreEquals = (value) => ({ field: 'genre', op: 'equals', value });
export const durationBetween = (min, max) => ({
  field: 'duration',
  op: 'range',
  min,
  max,
});
