// Offline QQ contract / fallback / queue regression tests: node --test tests/qq_source.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import * as qq from '../src/ui/sources/qq.mjs';
import { sourceOf, trackKey, withIdentity } from '../src/ui/sources/track.mjs';
const { AmPlayer, trackKey: playerKey } = createRequire(import.meta.url)('../src/ui/player.js');

const dto = {
  id: '0039MnYb0qxYhV', name: '晴天', artists: ['周杰伦'], album: '叶惠美', albumMid: '003DFRzD192KKD',
  durationMs: 269000, vip: true, ttml: true,
  file_url: 'https://example.invalid/private.mp3', cover: 'https://example.invalid/private.jpg',
};
const storage = () => {
  const data = new Map();
  return { getItem: (key) => data.get(key) ?? null, setItem: (key, value) => data.set(key, value) };
};

test('QQ DTO maps names, timing and badges and rebuilds same-origin artwork', () => {
  const track = qq.toTrack(dto);
  assert.equal(track.key, 'qq:0039MnYb0qxYhV');
  assert.equal(track.source, 'qq');
  assert.equal(track.kind, 'song');
  assert.equal(track.name, '晴天');
  assert.equal(track.artist, '周杰伦');
  assert.deepEqual(track.artists, [{ name: '周杰伦', href: '' }]);
  assert.equal(track.duration, 269000);
  assert.deepEqual(track.qq, { mid: dto.id, albumMid: dto.albumMid, vip: true, ttml: true });
  assert.equal(track.artwork, `/qq/cover/${dto.albumMid}?s={w}`);
  assert.equal(track.href, '');
  assert.equal(track.albumHref, '');
  assert.equal(JSON.stringify(track).includes('example.invalid'), false);
  assert.equal(JSON.stringify(track).includes('file_url'), false);
  const entry = qq.entryOf(track);
  assert.equal(entry.track, dto.id);
  assert.equal(entry.key, track.key);
  assert.equal(entry.artwork, `/qq/cover/${dto.albumMid}?s=600`);
});

test('QQ optional fields default safely; invalid IDs cannot form adapter paths', () => {
  const track = qq.toTrack({ id: '1', albumMid: '../bad', durationMs: -5 });
  assert.equal(track.duration, 0);
  assert.equal(track.artwork, '/qq/cover/1?s={w}');
  assert.equal(track.qq.vip, false);
  assert.equal(track.qq.ttml, false);
  assert.deepEqual(track.artists, []);
  for (const id of [undefined, null, '', '../secret', 'a?x=1', 'x'.repeat(33), '中']) {
    assert.throws(() => qq.toTrack({ id }));
    assert.throws(() => qq.playUrl(id));
    assert.throws(() => qq.downloadUrl(id));
  }
  assert.equal(qq.playUrl('1'), '/qq/song/1/play');
  assert.equal(qq.downloadUrl('1'), '/qq/song/1/download');
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

test('QQ clients only request local adapter paths and pass cancellation signals', async () => {
  const original = globalThis.fetch;
  const calls = [];
  const controller = new AbortController();
  globalThis.fetch = async (path, options) => {
    calls.push({ path, options });
    if (path.includes('/lyrics')) return { ok: true, json: async () => ({ candidates: [candidate('lrc')] }) };
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
  for (const name of ['closeQualityPanel', 'layout', 'emit', 'renderQueue', 'setLoading', 'renderToggle', 'renderTrackText', 'renderArtwork', 'renderMode', 'updateMediaSession', 'renderProgress', 'prepareNext', 'extendQueue', 'show']) player[name] = () => {};
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
