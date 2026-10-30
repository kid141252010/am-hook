// API requests use the local adapter; artwork uses public QQ CDN URLs from its DTOs.
import { trackKey, withIdentity } from './track.mjs';
import { artUrl } from '../library.mjs';
export { trackKey } from './track.mjs';
const MID = /^[0-9A-Za-z]{1,32}$/;
export const enabled = () => globalThis.window?.AM_HOOK_HOST?.qqEnabled === true;
const mid = (id) => { if (!['string', 'number'].includes(typeof id) || !MID.test(String(id))) throw new Error('Invalid QQ ID'); return String(id); };
export const QUALITY_IDS = ['MP3128', 'AAC48', 'WEB', 'AAC192', 'OGG96', 'OGG192', 'HQ', 'NAC',
  'SQ', 'RS', 'QAI', 'Q360V1', 'Q360V2', 'RA360', 'DTS', 'DTSX', 'DA', 'DAEAC3', 'DA448', 'DA1500'];
const DEFAULT_CODECS = {
  MP3128: 'mp3', AAC48: 'aac', WEB: 'aac', AAC192: 'aac', OGG96: 'vorbis', OGG192: 'vorbis', HQ: 'mp3', NAC: 'nac',
  SQ: 'flac', RS: 'flac', QAI: 'flac', Q360V1: 'flac', Q360V2: 'flac', RA360: 'unknown', DTS: 'dts', DTSX: 'dtsx',
  DA: 'ac-4', DAEAC3: 'ec-3', DA448: 'ac-4', DA1500: 'ac-4',
};
const CONTENT_TYPES = {
  mp3: 'audio/mpeg', aac: 'audio/mp4; codecs="mp4a.40.2"', flac: 'audio/flac', vorbis: 'audio/ogg; codecs="vorbis"',
  opus: 'audio/ogg; codecs="opus"', pcm: 'audio/wav', 'ec-3': 'audio/mp4; codecs="ec-3"',
};
/** WET stsd: DA/DA448/DA1500 = ac-4/dac4; DAEAC3 = ec-3/dec3 (2026-10-31).
 * QQ's fixed Dolby pipelines determine those codecs, including for saved queues.
 * Other tiers use codec metadata; tiers without decoders stay download-only. */
export function playbackInfo(quality = 'HQ', row = {}) {
  qualityId(quality);
  const dolby = ['DA', 'DA448', 'DA1500', 'DAEAC3'].includes(quality);
  const codec = !dolby && typeof row.codec === 'string' ? row.codec : DEFAULT_CODECS[quality];
  const playback = ['NAC', 'DTS', 'DTSX', 'RA360'].includes(quality) ? 'download'
    : codec === 'ec-3' ? 'joc' : Object.hasOwn(CONTENT_TYPES, codec) ? 'native' : 'download';
  return { playback, codec, codecs: codec, contentType: !dolby && typeof row.contentType === 'string' ? row.contentType
    : CONTENT_TYPES[codec] || (codec === 'ac-4' ? 'audio/mp4; codecs="ac-4"' : 'application/octet-stream') };
}
export function qualityId(value = 'HQ') {
  if (!QUALITY_IDS.includes(value)) throw new Error('Invalid QQ quality');
  return value;
}
const qualityQuery = (quality) => quality === undefined ? '' : `?quality=${qualityId(quality)}`;
export const songPath = (id) => `/qq-song/${mid(id)}`;
export const playUrl = (id, quality) => `/qq/song/${mid(id)}/play${qualityQuery(quality)}`;
export const downloadUrl = (id, quality) => `/qq/song/${mid(id)}/download${qualityQuery(quality)}`;
export function toTrack(dto) {
  const id = mid(dto.id);
  const albumMid = MID.test(dto.albumMid || '') ? dto.albumMid : id;
  const artists = (Array.isArray(dto.artists) ? dto.artists : []).filter((a) => typeof a === 'string').map((name) => ({ name, href: '' }));
  return withIdentity({
    source: 'qq', kind: 'song', id, name: String(dto.name || ''), title: String(dto.name || ''),
    artists, artist: artists.map((a) => a.name).join(' / '), album: String(dto.album || ''),
    albumId: albumMid, albumHref: '', href: songPath(id), country: '',
    duration: Number.isFinite(dto.durationMs) ? Math.max(0, dto.durationMs) : 0,
    artwork: { url: typeof dto.cover === 'string' ? dto.cover : '' }, explicit: false,
    qq: { mid: id, albumMid, vip: dto.vip === true, ttml: dto.ttml === true },
  });
}
export function entryOf(track) {
  return withIdentity({ ...track, track: track.id, artwork: artUrl(track.artwork, 600) });
}
export function qualityEntry(track, quality, row) {
  return { ...entryOf(track), quality: qualityId(quality), ...playbackInfo(quality, row) };
}
export function qualityItem(track, quality, row) {
  return { ...qualityEntry(track, quality, row), id: `qq:${track.id}:${quality}`, title: track.name,
    streamUrl: playUrl(track.id, quality), downloadUrl: downloadUrl(track.id, quality), label: `QQ · ${quality}`, variants: [] };
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
export function qualityRows(data, id, detect = globalThis.window?.AmHook?.detectQqModes) {
  const rows = Array.isArray(data?.qualities) ? data.qualities : [];
  return QUALITY_IDS.map((quality) => {
    const row = rows.find((item) => item?.quality === quality);
    const sizeBytes = Number.isSafeInteger(row?.sizeBytes) && row.sizeBytes >= 0 ? row.sizeBytes : null;
    const available = row?.available === true && sizeBytes > 0;
    const info = playbackInfo(quality, row);
    const playable = available && info.playback !== 'download' && (!detect || detect(info).length > 0);
    return { quality, available, playable, sizeBytes, ...info, format: typeof row?.format === 'string' ? row.format : '',
      ...(available ? { downloadUrl: downloadUrl(id, quality) } : {}),
      ...(playable ? { playUrl: playUrl(id, quality) } : {}) };
  });
}
export async function qualities(id, signal) {
  return qualityRows(await request(`/qq/song/${mid(id)}/qualities`, signal), id);
}
/** The detail page and full-screen lyrics share the parsed lyric model. */
export function lyricLines(result) {
  return (result?.song?.lines || []).filter((line) => typeof line.text === 'string' && line.text.trim())
    .map((line) => ({ text: line.text, translation: line.translation || '' }));
}
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
export function download(track, quality) {
  const link = document.createElement('a');
  link.href = downloadUrl(track.track ?? track.id, quality);
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
        album: raw.album, albumMid: raw.qq?.albumMid, cover: typeof raw.artwork === 'string' ? raw.artwork : raw.artwork?.url,
        durationMs: raw.duration, vip: raw.qq?.vip, ttml: raw.qq?.ttml });
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
