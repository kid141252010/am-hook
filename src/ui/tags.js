export const TAGS_PREF_KEY = 'am-hook:tags';

export const DEFAULT_TAGS_PREFS = {
  enabled: true,
  cover: true,
  lyrics: true,
  itunesIds: true,
};

export function loadTagsPrefs() {
  let raw = null;
  try {
    raw = localStorage.getItem(TAGS_PREF_KEY);
  } catch {
    return { ...DEFAULT_TAGS_PREFS };
  }
  if (!raw) return { ...DEFAULT_TAGS_PREFS };
  let parsed = null;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return { ...DEFAULT_TAGS_PREFS };
  }
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
    return { ...DEFAULT_TAGS_PREFS };
  }
  const out = { ...DEFAULT_TAGS_PREFS };
  for (const key of Object.keys(DEFAULT_TAGS_PREFS)) {
    if (typeof parsed[key] === 'boolean') out[key] = parsed[key];
  }
  return out;
}

export function saveTagsPrefs(prefs) {
  try {
    localStorage.setItem(TAGS_PREF_KEY, JSON.stringify(prefs ?? {}));
  } catch {
    // ignore (quota / disabled storage)
  }
}

export function artwork3000(template) {
  if (!template) return '';
  return String(template)
    .replace(/\{w\}/g, '3000')
    .replace(/\{h\}/g, '3000')
    .replace(/\{c\}/g, 'bb')
    .replace(/\{f\}/g, 'jpg');
}

function requestSignal(signal, timeoutMs = 10000) {
  const controller = new AbortController();
  const abort = () => controller.abort();
  if (signal?.aborted) abort();
  else signal?.addEventListener('abort', abort, { once: true });
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  return {
    signal: controller.signal,
    cleanup: () => {
      clearTimeout(timer);
      signal?.removeEventListener('abort', abort);
    },
  };
}

export async function fetchBytes(url, { signal, timeoutMs } = {}) {
  const request = requestSignal(signal, timeoutMs);
  try {
    const res = await fetch(url, { signal: request.signal });
    if (!res || !res.ok) return null;
    return new Uint8Array(await res.arrayBuffer());
  } catch {
    if (signal?.aborted) throw new DOMException('The operation was aborted', 'AbortError');
    return null;
  } finally {
    request.cleanup();
  }
}

/** 拉封面专用：只接受 JPEG（FF D8 魔数），否则返回 null 让调用方跳过 covr。 */
export async function fetchJpegBytes(url, options) {
  const bytes = await fetchBytes(url, options);
  if (!bytes || bytes.length < 2 || bytes[0] !== 0xff || bytes[1] !== 0xd8) return null;
  return bytes;
}

export async function fetchLyricsText(adamId, { signal, timeoutMs } = {}) {
  const request = requestSignal(signal, timeoutMs);
  try {
    const res = await fetch(`/lyrics/${encodeURIComponent(adamId)}`, { signal: request.signal });
    if (!res || !res.ok) return null;
    const body = await res.text();
    if (!body) return null;
    const { parseTTML } = await import('./lyrics/ttml.mjs');
    const parsed = parseTTML(body);
    if (!parsed || !Array.isArray(parsed.lines)) return null;
    const text = parsed.lines
      .map((l) => (l && l.text ? String(l.text) : '').trim())
      .filter(Boolean)
      .join('\n');
    return text || null;
  } catch {
    if (signal?.aborted) throw new DOMException('The operation was aborted', 'AbortError');
    return null;
  } finally {
    request.cleanup();
  }
}

function finiteOrUndefined(n) {
  return typeof n === 'number' && Number.isFinite(n) ? n : undefined;
}

function strOrUndefined(s) {
  if (typeof s !== 'string') return undefined;
  const t = s.trim();
  return t ? t : undefined;
}

export function buildSongTags(meta, opts) {
  const m = meta || {};
  const o = opts || {};
  const obj = {
    title: strOrUndefined(m.title),
    artist: strOrUndefined(m.artist),
    album: strOrUndefined(m.album),
    composer: strOrUndefined(m.composerName),
    genre: strOrUndefined(m.genre),
    releaseDate: strOrUndefined(m.releaseDate),
    copyright: strOrUndefined(m.copyright),
    trackNumber: finiteOrUndefined(m.trackNumber),
    trackTotal: finiteOrUndefined(m.trackCount),
    discNumber: finiteOrUndefined(m.discNumber),
    discTotal: finiteOrUndefined(m.discCount),
    mediaKind: 1,
    rating: m.explicit ? 4 : undefined,
    lyrics: strOrUndefined(o.lyrics),
    isrc: strOrUndefined(m.isrc),
    coverFormat: o.coverFormat || undefined,
  };

  if (o.includeItunesIds) {
    const trackId = m.id ? Number(m.id) : NaN;
    if (Number.isFinite(trackId)) obj.itunesTrackId = trackId;
    const playlistId = m.albumId ? Number(m.albumId) : NaN;
    if (Number.isFinite(playlistId)) obj.itunesPlaylistId = playlistId;
  }

  return JSON.stringify(obj);
}

export function buildMvTags(meta, opts) {
  const m = meta || {};
  const o = opts || {};
  const obj = {
    title: strOrUndefined(m.title),
    artist: strOrUndefined(m.artist),
    genre: strOrUndefined(m.genre),
    releaseDate: strOrUndefined(m.releaseDate),
    mediaKind: 6,
    coverFormat: o.coverFormat || undefined,
  };

  if (o.includeItunesIds) {
    const trackId = m.id ? Number(m.id) : NaN;
    if (Number.isFinite(trackId)) obj.itunesTrackId = trackId;
  }

  return JSON.stringify(obj);
}
