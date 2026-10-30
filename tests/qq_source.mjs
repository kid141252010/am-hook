// Offline QQ contract / fallback / queue regression tests: node --test tests/qq_source.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import * as qq from '../src/ui/sources/qq.mjs';
import { sourceOf, trackKey, withIdentity } from '../src/ui/sources/track.mjs';
import { artUrl } from '../src/ui/library.mjs';
const { AmPlayer, trackKey: playerKey, detectQqModes, useJocSettings } = createRequire(import.meta.url)('../src/ui/player.js');

const dto = {
  id: '0039MnYb0qxYhV', name: '晴天', artists: ['周杰伦'], album: '叶惠美', albumMid: '003DFRzD192KKD',
  durationMs: 269000, vip: true, ttml: true,
  file_url: 'https://example.invalid/private.mp3',
  cover: 'https://y.qq.com/music/photo_new/T002R{w}x{h}M000003DFRzD192KKD_5.jpg',
};
const storage = () => {
  const data = new Map();
  return { getItem: (key) => data.get(key) ?? null, setItem: (key, value) => data.set(key, value) };
};

test('QQ DTO maps names, timing, badges and direct QQ artwork', () => {
  const track = qq.toTrack(dto);
  assert.equal(track.key, 'qq:0039MnYb0qxYhV');
  assert.equal(track.source, 'qq');
  assert.equal(track.kind, 'song');
  assert.equal(track.name, '晴天');
  assert.equal(track.artist, '周杰伦');
  assert.deepEqual(track.artists, [{ name: '周杰伦', href: '' }]);
  assert.equal(track.duration, 269000);
  assert.deepEqual(track.qq, { mid: dto.id, albumMid: dto.albumMid, vip: true, ttml: true });
  assert.deepEqual(track.artwork, { url: dto.cover });
  assert.equal(track.href, `/qq-song/${dto.id}`);
  assert.equal(track.albumHref, '');
  assert.equal(JSON.stringify(track).includes('example.invalid'), false);
  assert.equal(JSON.stringify(track).includes('file_url'), false);
  const entry = qq.entryOf(track);
  assert.equal(entry.track, dto.id);
  assert.equal(entry.key, track.key);
  assert.equal(entry.artwork, 'https://y.qq.com/music/photo_new/T002R600x600M000003DFRzD192KKD_5.jpg');
  const opaque = { ...dto, cover: 'https://y.qq.com/cover.jpg?version=5' };
  assert.equal(qq.entryOf(qq.toTrack(opaque)).artwork, opaque.cover);
});

test('QQ optional fields default safely; invalid IDs cannot form adapter paths', () => {
  const track = qq.toTrack({ id: '1', albumMid: '../bad', durationMs: -5 });
  assert.equal(track.duration, 0);
  assert.deepEqual(track.artwork, { url: '' });
  assert.equal(track.qq.vip, false);
  assert.equal(track.qq.ttml, false);
  assert.deepEqual(track.artists, []);
  for (const id of [undefined, null, '', '../secret', 'a?x=1', 'x'.repeat(33), '中']) {
    assert.throws(() => qq.toTrack({ id }));
    assert.throws(() => qq.playUrl(id));
    assert.throws(() => qq.downloadUrl(id));
    assert.throws(() => qq.songPath(id));
  }
  assert.equal(qq.playUrl('1'), '/qq/song/1/play');
  assert.equal(qq.downloadUrl('1'), '/qq/song/1/download');
});

test('QQ quality rows list all tiers, retain formats and sizes, and omit unavailable actions', () => {
  const rows = qq.qualityRows({ qualities: [
    { quality: 'SQ', available: true, sizeBytes: 55397039, format: 'FLAC' },
    { quality: 'HQ', available: true, sizeBytes: 10792943, format: 'MP3 320 kbps' },
    { quality: 'WEB', available: false, sizeBytes: 0, format: 'MP3 128 kbps' },
    { quality: 'NAC', available: true, sizeBytes: 0, format: 'AAC' },
    { quality: 'unknown', available: true, sizeBytes: 42, format: 'ignored' },
  ] }, dto.id);
  assert.deepEqual(rows.map((row) => row.quality), ['MP3128', 'AAC48', 'WEB', 'AAC192', 'OGG96', 'OGG192', 'HQ', 'NAC',
    'SQ', 'RS', 'QAI', 'Q360V1', 'Q360V2', 'RA360', 'DTS', 'DTSX', 'DA', 'DAEAC3', 'DA448', 'DA1500']);
  const byQuality = Object.fromEntries(rows.map((row) => [row.quality, row]));
  assert.equal(byQuality.HQ.format, 'MP3 320 kbps');
  assert.equal(byQuality.HQ.sizeBytes, 10792943);
  assert.equal(byQuality.SQ.downloadUrl, `/qq/song/${dto.id}/download?quality=SQ`);
  assert.equal(byQuality.SQ.playUrl, `/qq/song/${dto.id}/play?quality=SQ`);
  for (const row of rows.filter((row) => !['HQ', 'SQ'].includes(row.quality))) {
    assert.equal(row.available, false);
    assert.equal(Object.hasOwn(row, 'playUrl'), false);
    assert.equal(Object.hasOwn(row, 'downloadUrl'), false);
  }
  assert.equal(byQuality.RS.sizeBytes, null);
  assert.equal(qq.qualityRows(null, dto.id).length, 20);
  for (const quality of ['', 'hq', '../SQ', 'HQ&x=1', null, 1]) {
    assert.throws(() => qq.playUrl(dto.id, quality));
    assert.throws(() => qq.downloadUrl(dto.id, quality));
    assert.throws(() => qq.qualityEntry(qq.toTrack(dto), quality));
  }
});

test('detail lyric lines use the best parsed source and retain translations as text', () => {
  const result = qq.selectLyrics([candidate('ttml', 'bad'), candidate('lrc', 'valid')], (text) => {
    if (text === 'bad') throw new Error('bad XML');
    return { lines: [{ text: '晴天 <script>', translation: 'Sunny day' }, { text: '  ' }] };
  });
  assert.equal(result.source, 'lrc');
  assert.deepEqual(qq.lyricLines(result), [{ text: '晴天 <script>', translation: 'Sunny day' }]);
  assert.deepEqual(qq.lyricLines({ song: null }), []);
});

test('source namespaces isolate equal numeric IDs and upgrade old Apple snapshots', () => {
  for (const input of [{ id: '123' }, { track: '123' }, { id: 'variant', track: '123' }, { id: '123', key: 'qq:forged' }]) {
    assert.equal(trackKey(input), 'apple:123');
    assert.equal(playerKey(input), trackKey(input));
    assert.equal(withIdentity(input).source, 'apple');
  }
  const apple = withIdentity({ id: '123' });
  const other = withIdentity({ source: 'qq', id: '123' });
  assert.notEqual(apple.key, other.key);
  assert.equal(playerKey(other), other.key);
  assert.equal(new Set([apple.key, other.key]).size, 2);
  assert.equal(sourceOf({ source: 'unknown' }), 'apple');
});

const candidate = (source, text = source) => ({ source, text });
const parse = (text) => {
  if (text === 'malformed') throw new Error('invalid XML');
  return { lines: text === 'empty' ? [] : [{ text }] };
};
test('QQ lyrics prefer AMLL, upstream TTML, QRC, then LRC regardless of response order', () => {
  const sources = ['lrc', 'qrc', 'ttml', 'amll'];
  for (const expected of ['amll', 'ttml', 'qrc', 'lrc']) {
    assert.equal(qq.selectLyrics(sources.map((s) => candidate(s)), parse).source, expected);
    sources.splice(sources.indexOf(expected), 1);
  }
});
test('missing, blank, malformed and untimed lyrics fall through without blocking LRC', () => {
  assert.equal(qq.selectLyrics([candidate('amll', 'malformed'), candidate('ttml', 'empty'), candidate('qrc', ' '), candidate('lrc')], parse).source, 'lrc');
  assert.equal(qq.selectLyrics([candidate('ttml', null), candidate('qrc', {}), candidate('lrc')], parse).source, 'lrc');
  assert.deepEqual(qq.selectLyrics(undefined, parse), { song: null });
  assert.deepEqual(qq.selectLyrics([candidate('apple'), candidate('unknown')], parse), { song: null });
});

test('QQ favorites persist independently, deduplicate keys and discard upstream fields', () => {
  const local = storage();
  local.setItem('am-hook:library', 'Apple snapshot');
  const track = qq.toTrack(dto);
  assert.equal(qq.toggleFavorite({ ...track, file_url: 'https://example.invalid/private' }, local), true);
  assert.equal(qq.favorites(local)[0].key, track.key);
  assert.deepEqual(qq.favorites(local)[0].artwork, { url: dto.cover });
  assert.equal(local.getItem('am-hook:qq-favorites').includes('file_url'), false);
  assert.equal(local.getItem('am-hook:library'), 'Apple snapshot');
  local.setItem('am-hook:qq-favorites', JSON.stringify([track, track, { source: 'apple', id: track.id }, { source: 'qq', id: '../x' }]));
  assert.equal(qq.favorites(local).length, 1);
  assert.equal(qq.toggleFavorite(track, local), false);
  assert.deepEqual(qq.favorites(local), []);
  local.setItem('am-hook:qq-favorites', '{broken');
  assert.deepEqual(qq.favorites(local), []);
  assert.deepEqual(qq.favorites({ getItem() { throw new Error('storage blocked'); } }), []);
});

test('QQ covers resolve for rows, legacy favorites and player entries; Apple templates retain their shape', () => {
  const track = qq.toTrack(dto);
  const resolved = (size) => dto.cover.replace('{w}', size).replace('{h}', size);
  assert.equal(artUrl(track.artwork, 80), resolved(80));
  const local = storage();
  local.setItem('am-hook:qq-favorites', JSON.stringify([{ ...track, artwork: dto.cover }]));
  assert.deepEqual(qq.favorites(local)[0].artwork, { url: dto.cover });
  assert.equal(artUrl(qq.favorites(local)[0].artwork, 96), resolved(96));
  assert.equal(qq.entryOf(track).artwork, resolved(600));
  assert.equal(qq.entryOf({ ...track, artwork: dto.cover }).artwork, resolved(600));
  const apple = 'https://example.com/{w}x{h}{c}.{f}';
  assert.equal(artUrl(apple, 80, 60), 'https://example.com/80x60bb.jpg');
  assert.equal(artUrl({ url: apple }, 80, 60), 'https://example.com/80x60bb.jpg');
});

test('QQ clients only request local adapter paths and pass cancellation signals', async () => {
  const original = globalThis.fetch;
  const calls = [];
  const controller = new AbortController();
  globalThis.fetch = async (path, options) => {
    calls.push({ path, options });
    if (path.includes('/lyrics')) return { ok: true, json: async () => ({ candidates: [candidate('lrc')] }) };
    if (path.includes('/qualities')) return { ok: true, json: async () => ({ qualities: [{ quality: 'HQ', available: true, sizeBytes: 10792943, format: 'MP3 320 kbps' }] }) };
    return { ok: true, json: async () => path.includes('/search') ? { songs: [dto], total: 99 } : dto };
  };
  try {
    const result = await qq.searchSongs('晴天 & 风', { limit: 10, offset: 20, signal: controller.signal });
    assert.equal(result.total, 99);
    assert.equal(result.songs[0].key, `qq:${dto.id}`);
    assert.equal(new URL(calls[0].path, 'https://example.invalid').searchParams.get('q'), '晴天 & 风');
    assert.equal(calls[0].options.signal, controller.signal);
    assert.equal((await qq.song(dto.id)).key, `qq:${dto.id}`);
    assert.equal((await qq.lyrics(dto.id, parse)).source, 'lrc');
    assert.equal((await qq.qualities(dto.id, controller.signal)).find((row) => row.quality === 'HQ').available, true);
    assert.equal(calls.at(-1).options.signal, controller.signal);
    assert.ok(calls.every(({ path }) => path.startsWith('/qq/')));
    globalThis.fetch = async () => ({ ok: false, status: 503, json() { throw new Error('must not read upstream body'); } });
    await assert.rejects(qq.searchSongs('x'), /QQ HTTP 503/);
  } finally { globalThis.fetch = original; }
});

// Small transport harness, using the real queue/source/start/tryMode methods.
function playerHarness() {
  const player = Object.create(AmPlayer.prototype);
  Object.assign(player, {
    source: 'apple', current: null, queue: null, sourceContexts: new Map(), playToken: 0, queueSerial: 0, nextSerial: 0,
    shuffle: false, repeat: 'off', root: { hidden: true }, unsupportedListeners: new Set(),
    audio: { paused: true, currentTime: 0, duration: 269, async play() { this.paused = false; }, pause() { this.paused = true; }, load() {}, addEventListener() {} },
  });
  player.clock = player.audio;
  for (const name of ['closeQualityPanel', 'layout', 'emit', 'renderQueue', 'renderModes', 'setLoading', 'renderToggle', 'renderTrackText', 'renderArtwork', 'renderMode', 'updateMediaSession', 'renderProgress', 'prepareNext', 'extendQueue', 'show']) player[name] = () => {};
  player.showError = (error) => { player.error = error; };
  player.teardown = () => { player.audio.pause(); };
  return player;
}
test('QQ plays the local 302 route through native audio without asking Apple for a master', async () => {
  const player = playerHarness();
  await player.playQueue([qq.entryOf(qq.toTrack(dto))]);
  assert.equal(player.current.source, 'qq');
  assert.equal(player.current.mode, 'native');
  assert.equal(player.current.key, `qq:${dto.id}`);
  assert.equal(player.audio.src, `/qq/song/${dto.id}/play`);
  assert.equal(player.audio.paused, false);
});

test('every native QQ quality reaches the play URL and matching playback label', async () => {
  const track = qq.toTrack(dto);
  for (const quality of qq.QUALITY_IDS.filter((quality) => qq.playbackInfo(quality).playback === 'native')) {
    const player = playerHarness();
    await player.playQueue([qq.qualityEntry(track, quality)]);
    assert.equal(player.current.quality, quality);
    assert.equal(player.current.id, `qq:${dto.id}:${quality}`);
    assert.equal(player.audio.src, qq.playUrl(dto.id, quality));
    assert.equal(player.modeText(), `QQ · ${quality}`);
    assert.equal(player.current.href, qq.songPath(dto.id));
    assert.equal(player.queue.entries[0].quality, quality);
  }
});

test('changing QQ quality on its detail page preserves the remaining queue and repeats that quality', async () => {
  const player = playerHarness();
  const track = qq.toTrack(dto);
  const next = qq.entryOf(qq.toTrack({ id: 'next', name: 'Next' }));
  await player.playQueue([qq.entryOf(track), next]);
  const queue = player.queue;
  await player.play(qq.qualityItem(track, 'SQ'));
  assert.equal(player.audio.src, qq.playUrl(dto.id, 'SQ'));
  assert.equal(player.queue, queue);
  assert.equal(queue.entries[1].track, 'next');
  assert.equal(queue.entries[0].quality, 'SQ');
  await player.playAt(0);
  assert.equal(player.current.quality, 'SQ');
  assert.equal(player.audio.src, qq.playUrl(dto.id, 'SQ'));
});

test('a first queued QQ song is loaded paused and a saved pending queue is accessible on switching source', async () => {
  const player = playerHarness();
  player.source = 'qq';
  player.appendQueue(qq.entryOf(qq.toTrack(dto)));
  await new Promise(setImmediate);
  assert.equal(player.current.track, dto.id);
  assert.equal(player.audio.paused, true);
  assert.equal(player.queue.pos, 0);
  assert.equal(player.queue.entries.length, 1);

  const other = playerHarness();
  other.appendQueue(qq.qualityEntry(qq.toTrack(dto), 'AAC48'));
  assert.equal(other.current, null);
  other.switchSource('qq');
  await new Promise(setImmediate);
  assert.equal(other.current.quality, 'AAC48');
  assert.equal(other.audio.paused, true);
  assert.equal(other.audio.src, qq.playUrl(dto.id, 'AAC48'));
});

test('adding QQ to its saved queue preserves Apple playback and source restoration retains quality', async () => {
  const player = playerHarness();
  const appleQueue = { entries: [{ track: '123' }], pos: 0 };
  player.current = { track: '123', id: '123:stereo', source: 'apple' };
  player.queue = appleQueue;
  player.audio.paused = false;
  const entry = qq.qualityEntry(qq.toTrack(dto), 'SQ');
  player.appendQueue(entry);
  assert.equal(player.source, 'apple');
  assert.equal(player.queue, appleQueue);
  assert.equal(player.current.track, '123');
  assert.equal(player.audio.paused, false);
  assert.equal(player.sourceContexts.get('qq').queue.entries[0].quality, 'SQ');
  await player.playQueue([entry]);
  player.audio.currentTime = 23;
  player.switchSource('apple');
  player.switchSource('qq');
  await new Promise(setImmediate);
  assert.equal(player.current.quality, 'SQ');
  assert.equal(player.current.label, 'QQ · SQ');
  assert.equal(player.audio.src, qq.playUrl(dto.id, 'SQ'));
  assert.equal(player.audio.paused, true);
});
test('source switching saves each queue, position and progress; mixed input is filtered', async () => {
  const player = playerHarness();
  const appleQueue = { entries: [{ track: '123' }], pos: 0 };
  player.queue = appleQueue;
  player.current = { track: '123', id: '123:stereo', source: 'apple' };
  player.audio.currentTime = 42;
  await player.playQueue([{ track: '123' }, qq.entryOf(qq.toTrack({ id: '123' }))], 1);
  assert.deepEqual(player.queue.entries.map((entry) => entry.key), ['qq:123']);
  const qqQueue = player.queue;
  player.audio.currentTime = 18;
  const starts = [];
  player.start = (item, options) => { starts.push({ item, options }); player.current = item; };
  player.switchSource('apple');
  assert.equal(player.queue, appleQueue);
  assert.equal(starts[0].options.resumeAt, 42);
  assert.equal(starts[0].options.paused, true);
  player.switchSource('qq');
  assert.equal(player.queue, qqQueue);
  assert.equal(starts[1].options.resumeAt, 18);
  assert.equal(starts[1].item.key, 'qq:123');
});

test('restoring a native QQ context loads metadata without autoplay', async () => {
  const player = playerHarness();
  let plays = 0;
  player.audio.play = async () => { plays++; };
  await player.start({ ...qq.entryOf(qq.toTrack(dto)), id: 'qq:test:HQ', streamUrl: qq.playUrl(dto.id) }, { paused: true, resumeAt: 12 });
  assert.equal(plays, 0);
  assert.equal(player.current.source, 'qq');
  assert.equal(player.audio.src, qq.playUrl(dto.id));
});

test('switching source invalidates a pending Apple resolution before it can replace QQ context', async () => {
  const player = playerHarness();
  const original = globalThis.AmWrapper;
  let finish;
  globalThis.AmWrapper = { songMaster: () => new Promise((resolve) => { finish = resolve; }) };
  try {
    const pending = player.playQueue([{ track: '123', name: 'Apple song' }]);
    assert.equal(player.pendingKey, 'apple:123');
    player.switchSource('qq');
    finish({ variants: [] });
    await pending;
    assert.equal(player.source, 'qq');
    assert.equal(player.current, null);
    assert.equal(player.pendingKey, null);
    assert.equal(player.queue, null);
  } finally { globalThis.AmWrapper = original; }
});


test('AAC/OGG and multichannel FLAC tiers are native; all four Dolby tiers select JOC', () => {
  for (const quality of ['AAC48', 'WEB', 'AAC192', 'OGG96', 'OGG192', 'QAI', 'Q360V1', 'Q360V2']) {
    assert.equal(qq.playbackInfo(quality).playback, 'native', quality);
  }
  for (const quality of ['DA', 'DAEAC3', 'DA448', 'DA1500']) {
    assert.equal(qq.playbackInfo(quality).playback, 'joc', quality);
    assert.equal(qq.playbackInfo(quality, { codec: 'ac-4' }).playback, 'download', quality);
  }
  for (const quality of ['NAC', 'DTS', 'DTSX', 'RA360']) {
    assert.equal(qq.playbackInfo(quality, { codec: 'flac' }).playback, 'download', quality);
  }
  assert.equal(qq.playbackInfo('Q360V1', { codec: 'ac-4' }).playback, 'download');
  assert.equal(qq.playbackInfo('QAI', { codec: 'ec-3' }).playback, 'joc');
});

test('QQ rows keep downloads when codecs or browser capabilities prevent playback', () => {
  const data = { qualities: ['NAC', 'DTS', 'DTSX', 'RA360', 'DA', 'OGG96', 'AAC48', 'QAI'].map((quality) => ({
    quality, available: true, sizeBytes: 1024, ...(quality === 'DA' ? { codec: 'ac-4' } : {}),
  })) };
  const rows = qq.qualityRows(data, dto.id, (info) => detectQqModes(info, { canPlayType: (mime) => mime.startsWith('audio/mp4') ? 'maybe' : '' }));
  for (const row of rows.filter((row) => row.available)) {
    assert.ok(row.downloadUrl);
    assert.equal(row.playable, row.quality === 'AAC48', row.quality);
    assert.equal(Object.hasOwn(row, 'playUrl'), row.playable);
  }
});

test('QQ JOC capability detection is independent of Apple decryption and honors binaural settings', () => {
  const saved = { Worker: globalThis.Worker, AudioContext: globalThis.AudioContext, AmDecrypt: globalThis.AmDecrypt };
  globalThis.Worker = function Worker() {};
  globalThis.AudioContext = function AudioContext() {};
  globalThis.AmDecrypt = undefined;
  try {
    useJocSettings({ get: () => ({ mode: 'binaural' }) });
    assert.deepEqual(detectQqModes(qq.playbackInfo('DA448')), ['jocbin', 'joc2']);
    useJocSettings({ get: () => ({ mode: 'off' }) });
    assert.deepEqual(detectQqModes(qq.playbackInfo('DA1500')), ['joc2']);
    globalThis.Worker = undefined;
    assert.deepEqual(detectQqModes(qq.playbackInfo('DA')), []);
  } finally { Object.assign(globalThis, saved); useJocSettings(null); }
});

test('download-only QQ entries cannot start audio', async () => {
  for (const quality of ['NAC', 'DTS', 'DTSX', 'RA360']) {
    const player = playerHarness();
    await player.playQueue([qq.qualityEntry(qq.toTrack(dto), quality)]);
    assert.equal(player.current, null);
    assert.equal(player.audio.src, undefined);
    assert.ok(player.error);
  }
});

test('queue repeats preserve an actual payload override when changing quality', async () => {
  const player = playerHarness();
  const track = qq.toTrack(dto);
  await player.playQueue([qq.entryOf(track)]);
  await player.play(qq.qualityItem(track, 'Q360V1', { codec: 'vorbis', contentType: 'audio/ogg' }));
  await player.playAt(0);
  assert.equal(player.current.codec, 'vorbis');
  assert.equal(player.current.contentType, 'audio/ogg');
  assert.equal(player.current.playback, 'native');
});
