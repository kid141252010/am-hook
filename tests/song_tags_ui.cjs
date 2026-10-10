// Run with: node tests/song_tags_ui.cjs <path-to-playwright-package>
// Uses a local song fixture to cover metadata-menu keyboard access and cancellation.
// Set AM_HOOK_BROWSER=msedge when Chrome is not installed.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require(process.argv[2] || 'playwright');

const root = path.join(__dirname, '../src/ui');

function serveAsset(route, url) {
  const file = url.pathname.startsWith('/assets/') ? url.pathname.slice('/assets/'.length) : 'app.html';
  const full = path.join(root, file);
  if (!fs.existsSync(full)) return route.fulfill({ status: 404, body: '' });
  const contentType = file.endsWith('.css') ? 'text/css'
    : file.endsWith('.wasm') ? 'application/wasm'
    : /\.m?js$/.test(file) ? 'text/javascript'
    : 'text/html';
  return route.fulfill({ body: fs.readFileSync(full), contentType });
}

(async () => {
  const browser = await chromium.launch({
    channel: process.env.AM_HOOK_BROWSER || 'chrome',
    headless: true,
  });
  try {
    const page = await browser.newPage();
    await page.addInitScript(() => localStorage.setItem('am-hook:lang', 'en'));

    let lyricsRoute;
    let lyricsStarted;
    const lyricsRequested = new Promise(resolve => { lyricsStarted = resolve; });
    await page.route('**/*', route => {
      const url = new URL(route.request().url());
      if (url.pathname === '/amp/v1/catalog/us/songs/123') {
        return route.fulfill({ json: {
          data: [{ id: '123', type: 'songs', attributes: { name: 'Review song', artistName: 'Artist' } }],
        } });
      }
      if (url.pathname.startsWith('/amp/')) return route.fulfill({ status: 404, json: { errors: [] } });
      if (url.pathname === '/status') return route.fulfill({ json: { code: 0, regions: ['us'] } });
      if (url.pathname.startsWith('/parse/song/')) {
        return route.fulfill({ json: {
          masterUrl: 'https://fixture.test/master.m3u8',
          hook: false,
          variants: [{ group_id: 'aac', codecs: 'mp4a.40.2', channels: '2', uri: 'track.m3u8', file_uri: 'track.mp4' }],
        } });
      }
      if (url.pathname === '/slow') return;
      if (url.pathname.startsWith('/lyrics/')) {
        lyricsRoute = route;
        lyricsStarted();
        return;
      }
      if (url.hostname !== 'am.test') return route.abort();
      return serveAsset(route, url);
    });

    await page.goto('http://am.test/https://music.apple.com/us/song/_/123');
    await page.locator('#variant-list .variant').waitFor();
    await page.waitForFunction(() => document.querySelector('#title')?.textContent === 'Review song');

    // The shared request helper must time out optional metadata and propagate cancellation.
    const timeoutResult = await page.evaluate(async () => {
      const { fetchBytes } = await import('/assets/tags.js');
      return fetchBytes('/slow', { timeoutMs: 20 });
    });
    assert.equal(timeoutResult, null, 'optional bytes time out as a skipped tag');
    const abortResult = await page.evaluate(async () => {
      const { fetchBytes } = await import('/assets/tags.js');
      const controller = new AbortController();
      const pending = fetchBytes('/slow', { signal: controller.signal });
      controller.abort();
      try { await pending; return 'resolved'; } catch (error) { return error.name; }
    });
    assert.equal(abortResult, 'AbortError', 'caller cancellation reaches fetchBytes');

    const trigger = page.locator('#variant-list [aria-haspopup]').first();
    await trigger.click();
    assert.match(await page.evaluate(() => document.activeElement.textContent), /Download decrypted file/);
    await page.keyboard.press('ArrowDown');
    assert.equal(await page.evaluate(() => document.activeElement.getAttribute('role')), 'menuitemcheckbox');
    await page.keyboard.press('ArrowDown');
    const cover = page.locator('#menu input[type=checkbox]').nth(1);
    assert(await cover.evaluate(el => el === document.activeElement));
    const coverBefore = await cover.isChecked();
    await page.keyboard.press('Space');
    assert.equal(await cover.isChecked(), !coverBefore, 'Space toggles a focused tag checkbox');
    await page.keyboard.press('Enter');
    assert.equal(await cover.isChecked(), coverBefore, 'Enter toggles a focused tag checkbox');
    await page.keyboard.press('Tab');
    assert.equal(await page.locator('#menu').isHidden(), false, 'Tab moves within the open menu');
    await page.keyboard.press('Escape');
    assert(await page.locator('#menu').isHidden());

    await page.evaluate(() => {
      window.__downloadCalled = false;
      window.AmDecrypt.openTrack = async () => ({ size: 100 });
      window.AmDecrypt.download = async () => {
        window.__downloadCalled = true;
        throw new Error('unexpected download');
      };
    });
    await trigger.click();
    await page.locator('#menu [role=menuitem]').first().click();
    await lyricsRequested;
    await page.locator('.dl-cancel').click();
    await page.locator('.dl-cancel').waitFor({ state: 'hidden' });
    assert.equal(await page.evaluate(() => window.__downloadCalled), false, 'cancel prevents the media download');
    try { await lyricsRoute?.fulfill({ status: 404, body: '' }); } catch { /* fetch was already aborted */ }

    console.log('Song tag UI: timeout, AbortSignal, keyboard menu, and pending-lyrics cancellation passed.');
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
