// API requests use the local adapter; artwork uses public QQ CDN URLs from its DTOs.
import { trackKey, withIdentity } from './track.mjs';
export { trackKey } from './track.mjs';
const MID = /^[0-9A-Za-z]{1,32}$/;
export const enabled = () => globalThis.window?.AM_HOOK_HOST?.qqEnabled === true;
const mid = (id) => { if (!['string', 'number'].includes(typeof id) || !MID.test(String(id))) throw new Error('Invalid QQ ID'); return String(id); };
export const playUrl = (id) => `/qq/song/${mid(id)}/play`;
export const downloadUrl = (id) => `/qq/song/${mid(id)}/download`;
export function toTrack(dto) {
  const id = mid(dto.id);
  const albumMid = MID.test(dto.albumMid || '') ? dto.albumMid : id;
  const artists = (Array.isArray(dto.artists) ? dto.artists : []).filter((a) => typeof a === 'string').map((name) => ({ name, href: '' }));
  return withIdentity({
    source: 'qq', kind: 'song', id, name: String(dto.name || ''), title: String(dto.name || ''),
    artists, artist: artists.map((a) => a.name).join(' / '), album: String(dto.album || ''),
    albumId: albumMid, albumHref: '', href: '', country: '',
    duration: Number.isFinite(dto.durationMs) ? Math.max(0, dto.durationMs) : 0,
    artwork: typeof dto.cover === 'string' ? dto.cover : '', explicit: false,
    qq: { mid: id, albumMid, vip: dto.vip === true, ttml: dto.ttml === true },
  });
}
export function entryOf(track) {
  return withIdentity({ ...track, track: track.id, artwork: track.artwork.replaceAll('{w}', '600').replaceAll('{h}', '600') });
}
async function request(path, signal) {
  const response = await fetch(path, { signal });
  if (!response.ok) throw new Error(`QQ HTTP ${response.status}`);
  return response.json();
}
export async function searchSongs(q, { limit = 30, offset = 0, signal } = {}) {
  const data = await request(`/qq/search?${new URLSearchParams({ q, limit, offset })}`, signal);
  return { songs: (data.songs || []).map(toTrack), total: data.total || 0 };
}
export async function song(id, signal) { return toTrack(await request(`/qq/song/${mid(id)}`, signal)); }
export const LYRIC_PRIORITY = ['amll', 'ttml', 'qrc', 'lrc'];
// Try the next source if a candidate is empty, malformed or has no timed lines.
export function selectLyrics(candidates, parse) {
  for (const source of LYRIC_PRIORITY) {
    for (const candidate of candidates || []) {
      if (candidate.source !== source || typeof candidate.text !== 'string' || !candidate.text.trim()) continue;
      try {
        const song = parse(candidate.text);
        if (song?.lines?.length) return { source, text: candidate.text, song, authors: [], amllStatus: null };
      } catch { /* fall through to the next candidate */ }
    }
  }
  return { song: null };
}
export async function lyrics(id, parse, signal) {
  const data = await request(`/qq/song/${mid(id)}/lyrics`, signal);
  return selectLyrics(data.candidates, parse);
}
export function download(track) {
  const link = document.createElement('a');
  link.href = downloadUrl(track.id ?? track.track);
  link.download = '';
  document.body.append(link); link.click(); link.remove();
}
const FAVORITES = 'am-hook:qq-favorites';
const listeners = new Set();
export function favorites(storage) {
  try {
    storage ||= globalThis.localStorage;
    const records = JSON.parse(storage.getItem(FAVORITES) || '[]');
    if (!Array.isArray(records)) return [];
    const unique = new Map();
    for (const raw of records) {
      if (raw?.source !== 'qq' || !MID.test(String(raw.id))) continue;
      // Rebuild snapshots through the same DTO allowlist; never persist audio URLs.
      const track = toTrack({ id: raw.id, name: raw.name, artists: raw.artists?.map((a) => a.name),
        album: raw.album, albumMid: raw.qq?.albumMid, cover: raw.artwork, durationMs: raw.duration, vip: raw.qq?.vip, ttml: raw.qq?.ttml });
      unique.set(track.key, track);
    }
    return [...unique.values()];
  } catch { return []; }
}
export const isFavorite = (track) => favorites().some((t) => t.key === trackKey(track));
export function toggleFavorite(track, storage = globalThis.localStorage) {
  const records = favorites(storage);
  const key = trackKey(track);
  const found = records.findIndex((t) => t.key === key);
  if (found >= 0) records.splice(found, 1); else records.unshift(withIdentity(track));
  // Read once more through the allowlist before writing; storage failures reach the UI.
  const clean = favorites({ getItem: () => JSON.stringify(records) });
  storage.setItem(FAVORITES, JSON.stringify(clean));
  for (const fn of listeners) fn();
  return found < 0;
}
export function onFavoritesChange(fn) { listeners.add(fn); return () => listeners.delete(fn); }
globalThis.addEventListener?.('storage', (e) => { if (e.key === FAVORITES || e.key === null) for (const fn of listeners) fn(); });
