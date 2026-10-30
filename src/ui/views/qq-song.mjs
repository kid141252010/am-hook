import * as qq from '../sources/qq.mjs';
import { trackKey } from '../sources/track.mjs';
import { artUrl } from '../library.mjs';
import { parseTTML } from '../lyrics/ttml.mjs';
import { createDetailHeader } from './detail-header.mjs';

export const bodyClass = 'song-page qq-song-page';
export const styles = ['/assets/views/song.css', '/assets/views/qq-song.css'];

export function mount({ root, url, signal, player, onLangChange, toast }) {
  const { t } = window.AmI18n;
  const { formatTime } = window.AmHook;
  const $ = (id) => root.querySelector(`#${id}`);
  const mid = url.pathname.match(/^\/qq-song\/([0-9A-Za-z]{1,32})\/?$/)?.[1];
  let track = null;
  let rows = qq.qualityRows(null, mid);
  let qualityState = 'loading';
  let lyric = { song: null };
  let errorKey = '';
  const el = (tag, props, ...children) => {
    const node = Object.assign(document.createElement(tag), props);
    node.append(...children);
    return node;
  };
  const header = createDetailHeader({ $, signal, artUrl, rerender: () => renderHeader() });
  const active = (quality) => !!track && trackKey(player.current) === track.key && player.current?.quality === quality;
  const playing = () => !!player.current && !player.transport().paused;

  function renderFavorite() {
    const saved = track && qq.isFavorite(track);
    $('favorite').disabled = !track;
    $('favorite').textContent = t(saved ? 'library.unfavorite' : 'library.favorite');
    $('favorite').setAttribute('aria-pressed', String(!!saved));
  }

  function renderHeader() {
    document.title = `${track?.name || t('qq.songTitle')} · am-hook`;
    if (!track) return;
    $('title').textContent = track.name;
    $('subtitle').textContent = track.artist;
    $('meta').textContent = [track.album, track.duration ? formatTime(track.duration / 1000) : ''].filter(Boolean).join(' · ');
    $('badges').replaceChildren(el('span', { className: 'qq-badge', textContent: t('source.qq') }),
      ...(track.qq.vip ? [el('span', { className: 'qq-badge qq-vip', textContent: t('qq.vip') })] : []));
    header({ cover: track.artwork, attrs: {}, alt: t('song.coverAlt', { title: track.name }) });
    $('enqueue').disabled = false;
    $('enqueue').textContent = t('qq.addQueue');
    renderFavorite();
    syncPlaying();
  }

  function playQuality(quality) {
    if (!track || !rows.find((row) => row.quality === quality)?.available) return;
    if (active(quality)) { player.toggle(); return; }
    player.play(qq.qualityItem(track, quality));
  }

  function syncPlaying() {
    $('play-best').disabled = !track || !rows.find((row) => row.quality === 'HQ')?.available;
    $('play-best').textContent = t(active('HQ') && playing() ? 'song.pause' : 'song.play');
    for (const node of $('variant-list').children) {
      const current = active(node.dataset.quality);
      node.classList.toggle('playing', current);
      node.classList.toggle('paused', current && !playing());
      const button = node.querySelector('button');
      if (button) button.textContent = t(current && playing() ? 'qq.pauseQuality' : 'qq.playQuality');
    }
  }

  function renderQualities() {
    $('quality-status').textContent = qualityState === 'loading' ? t('qq.qualitiesLoading')
      : qualityState === 'failed' ? t('qq.qualitiesFailed') : '';
    $('variant-list').replaceChildren(...rows.map((row) => {
      const format = row.format === 'Lossless' ? t('qq.formatLossless')
        : row.format === 'Web audio' ? t('qq.formatWeb') : row.format || t('qq.formatUnknown');
      const spec = [format,
        row.sizeBytes > 0 ? t('qq.sizeMb', { size: (row.sizeBytes / 1_000_000).toFixed(2) }) : '',
        !row.available ? t('qq.unavailable') : ''].filter(Boolean).join(' · ');
      const node = el('div', { className: `variant qq-quality${row.available ? '' : ' unavailable'}` },
        el('div', { className: 'variant-main' },
          el('div', { className: 'variant-name', textContent: row.quality }),
          el('div', { className: 'variant-spec', textContent: spec })));
      node.dataset.quality = row.quality;
      node.setAttribute('aria-disabled', String(!row.available));
      if (row.available) {
        const play = el('button', { className: 'song-reparse', type: 'button', disabled: !track, textContent: t('qq.playQuality') });
        play.setAttribute('aria-label', t('qq.playQualityAria', { quality: row.quality }));
        play.addEventListener('click', () => playQuality(row.quality));
        const download = el('a', { className: 'song-reparse qq-quality-download', href: row.downloadUrl, download: '', textContent: t('qq.downloadQuality') });
        download.setAttribute('aria-label', t('qq.downloadQualityAria', { quality: row.quality }));
        node.append(play, download);
      }
      return node;
    }));
    syncPlaying();
  }

  function renderLyrics() {
    const lines = qq.lyricLines(lyric);
    $('lyrics').hidden = !lines.length;
    $('lyrics-source').textContent = lyric.source ? t('qq.lyricsSource', { source: lyric.source.toUpperCase() }) : '';
    $('lyrics-lines').replaceChildren(...lines.map((line) => el('p', { className: 'qq-lyric-line' },
      el('span', { textContent: line.text }),
      ...(line.translation ? [el('span', { className: 'qq-lyric-translation', textContent: line.translation })] : []))));
  }

  function render() {
    renderHeader(); renderQualities(); renderLyrics();
    $('alert').hidden = !errorKey;
    $('alert').textContent = errorKey ? t(errorKey) : '';
  }
  $('play-best').addEventListener('click', () => playQuality('HQ'));
  $('favorite').addEventListener('click', () => {
    if (!track) return;
    try { qq.toggleFavorite(track); } catch { toast(t('qq.storageFailed')); }
  });
  $('enqueue').addEventListener('click', () => {
    if (!track) return;
    player.appendQueue(qq.entryOf(track));
    toast(t('qq.queued'));
  });
  player.onChange(syncPlaying);
  signal.addEventListener('abort', qq.onFavoritesChange(renderFavorite), { once: true });
  onLangChange(render);
  render();

  // Metadata, qualities and lyrics fail independently; a missing lyric never hides the song.
  qq.song(mid, signal).then((value) => {
    if (signal.aborted) return;
    track = value; render();
  }).catch(() => { if (!signal.aborted) { errorKey = 'qq.songFailed'; render(); } });
  qq.qualities(mid, signal).then((value) => {
    if (signal.aborted) return;
    rows = value; qualityState = 'ready'; renderQualities();
  }).catch(() => { if (!signal.aborted) { qualityState = 'failed'; renderQualities(); } });
  qq.lyrics(mid, parseTTML, signal).then((value) => {
    if (signal.aborted) return;
    lyric = value; renderLyrics();
  }).catch(() => { /* Absent or unavailable lyrics omit the entire section. */ });
}
