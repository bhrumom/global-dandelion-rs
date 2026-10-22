// Actions-only: real Wasm/WebGL, with an explicitly labelled native UI fixture.
const { chromium } = require('playwright');
const { readFile } = require('node:fs/promises');
const { resolve, sep, extname } = require('node:path');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const resources = resolve(process.argv[2]);
const origin = 'https://global-dandelion.fixture.invalid';
(async () => {
  const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader'] });
  const context = await browser.newContext({ acceptDownloads: true });
  const external = [], errors = []; let count = 0;
  const check = (actual, expected) => { assert.deepEqual(actual, expected); count++; };
  const mime = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.wasm': 'application/wasm' };
  await context.route('**/*', async route => {
    const url = new URL(route.request().url());
    if (url.origin !== origin) { external.push(url.origin); return route.abort(); }
    const path = resolve(resources, '.' + decodeURIComponent(url.pathname));
    if (!path.startsWith(resources + sep)) return route.abort();
    try { await route.fulfill({ body: await readFile(path), contentType: mime[extname(path)] || 'application/octet-stream' }); }
    catch { await route.fulfill({ status: 404, body: '' }); }
  });
  await context.setOffline(true);
  const page = await context.newPage(); page.on('pageerror', error => errors.push(error.message));
  const ready = () => page.waitForFunction(() => document.getElementById('composer-status').textContent.includes('点击 ↑'));
  const complete = () => page.waitForFunction(() => /本地回环完成|引擎已完成/.test(document.getElementById('composer-status').textContent));
  try {
    await page.goto(origin + '/index.html'); await ready();
    check(await page.locator('#advanced-workbench, #files, #start, #send, .workspace').count(), 0);
    check(await page.locator('#cycles').getAttribute('type'), 'checkbox');
    check(await page.locator('#composer-settings').isVisible(), false);
    await page.locator('#open-settings').click(); check(await page.locator('#composer-settings').isVisible(), true);
    await page.locator('#rate-local').fill('4');
    await page.locator('#origin-lat').fill('31.23'); await page.locator('#origin-lon').fill('121.47'); await page.locator('#set-origin').click();
    assert.match(await page.locator('#location-summary').textContent(), /31.230/); count++;
    await page.locator('#open-settings').click();
    await page.locator('#message').fill('愿一切安好');
    await page.locator('#composer-files').setInputFiles([{ name: '<img onerror=alert(1)>.txt', mimeType: 'text/plain', buffer: Buffer.alloc(4096, 65) }, { name: 'empty.txt', mimeType: 'text/plain', buffer: Buffer.alloc(0) }]);
    check(await page.locator('#composer-file-list img').count(), 0);
    const started = Date.now(); await page.locator('#launch').click(); await complete();
    assert.ok(Date.now() - started >= 850, '4 KiB/s must pace 4 KiB payload'); count++;
    const downloadPromise = page.waitForEvent('download'); await page.locator('#composer-export').click();
    const report = JSON.parse(fs.readFileSync(await (await downloadPromise).path(), 'utf8'));
    check(report.verifiedBytes, 4096 + Buffer.byteLength('愿一切安好')); check(report.completedPasses, 1); check(report.networkSent, false);
    const rejected = await page.evaluate(async () => {
      const rust = await import('./pkg/dandelion_web_adapter.js');
      const frame = rust.encode_packet(new Uint8Array([1, 2]), new Uint8Array(16), 0, 0, true); let rejected = 0;
      for (const bad of [frame.slice(0, 20), new Uint8Array(2000), new Uint8Array([...frame, 0])]) { try { rust.verify_packet(bad); } catch { rejected++; } }
      frame[80] ^= 1; try { rust.verify_packet(frame); } catch { rejected++; } return rejected;
    }); check(rejected, 4);
    await page.locator('#clear-composer').click(); await page.locator('#message').fill('continuous'); await page.locator('#cycles').check(); await page.locator('#launch').click();
    await page.waitForFunction(() => parseInt(document.getElementById('composer-pass').textContent) >= 4);
    check(await page.locator('#stop-composer').isVisible(), true);
    assert.match(await page.locator('#composer-pass').textContent(), /∞/); count++;
    await page.screenshot({ path: 'miniapp-ui-globe-running.png', fullPage: true });
    await page.locator('#stop-composer').click(); check(await page.locator('#launch').isVisible(), true);
    await page.locator('#open-settings').click(); await page.locator('#rate-local').fill('0'); await page.locator('#open-settings').click();
    await page.locator('#launch').click(); await page.waitForFunction(() => document.getElementById('composer-status').textContent.includes('速度请输入'));
    check(await page.locator('#stop-composer').isVisible(), false);
    await page.locator('#open-settings').click(); await page.locator('#rate-local').fill('1024'); await page.locator('#open-settings').click();
    await page.locator('#mode').selectOption('global'); await page.locator('#message').fill('offline must not send'); await page.locator('#launch').click();
    await page.waitForFunction(() => document.getElementById('composer-status').textContent.includes('尚未安装'));
    check(await page.locator('#earth').getAttribute('data-operation'), 'idle');
    await page.locator('#mode').selectOption('local'); await page.locator('#cycles').uncheck();
    await page.locator('#composer-files').setInputFiles(Array.from({ length: 129 }, (_, i) => ({ name: 'item-' + i, mimeType: 'text/plain', buffer: Buffer.from('x') })));
    assert.match(await page.locator('#composer-status').textContent(), /超出限制/); count++;
    await page.reload(); await ready(); check(await page.locator('#composer-file-list li').count(), 0);
    await page.setViewportSize({ width: 390, height: 844 }); await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.locator('#open-settings').click(); check(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    await page.screenshot({ path: 'miniapp-ui-settings-mobile.png', fullPage: true });
    await page.locator('#open-settings').click(); await page.locator('#message').fill('mobile Rust'); await page.locator('#launch').click(); await complete();
    check(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    const pixel = await page.evaluate(() => { const gl = document.getElementById('earth').getContext('webgl'); const p = new Uint8Array(4); gl.readPixels(gl.drawingBufferWidth / 2, gl.drawingBufferHeight / 2, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, p); return p[3]; });
    assert.ok(pixel > 0, 'real WebGL sphere'); count++;
    await page.screenshot({ path: 'miniapp-ui-globe-mobile.png', fullPage: true });
    await page.setViewportSize({ width: 1180, height: 1000 }); await page.screenshot({ path: 'miniapp-ui-globe-desktop.png', fullPage: true });
    // Reproduce the installed Host policy: WebGL unavailable, real Canvas2D.
    const software = await context.newPage(); software.on('pageerror', error => errors.push(error.message));
    await software.addInitScript(() => {
      const original = HTMLCanvasElement.prototype.getContext;
      HTMLCanvasElement.prototype.getContext = function(kind, ...args) { return /webgl|experimental-webgl/.test(kind) ? null : original.call(this, kind, ...args); };
    });
    await software.goto(origin + '/index.html');
    await software.waitForFunction(() => document.getElementById('earth').dataset.renderer === 'software-3d' && document.getElementById('earth').getContext('2d').getImageData(document.getElementById('earth').width / 2, document.getElementById('earth').height / 2, 1, 1).data[3] > 0);
    check(await software.locator('#earth-fallback').isVisible(), false);
    const beforeRotation = await software.locator('#earth').evaluate(canvas => canvas.toDataURL());
    await software.locator('#earth').focus(); for (let i = 0; i < 5; i++) await software.keyboard.press('ArrowRight');
    await software.waitForFunction(previous => document.getElementById('earth').toDataURL() !== previous, beforeRotation); count++;
    await software.locator('#message').fill('software globe'); await software.locator('#cycles').check(); await software.locator('#launch').click();
    await software.waitForFunction(() => parseInt(document.getElementById('composer-pass').textContent) >= 2);
    check(await software.locator('#earth').getAttribute('data-operation'), 'running');
    await software.locator('#stop-composer').click(); check(await software.locator('#earth').getAttribute('data-operation'), 'idle');
    await software.screenshot({ path: 'miniapp-ui-software-globe.png', fullPage: true }); await software.close();
    // Native transport below is a UI fixture, not real Internet delivery.
    await context.addInitScript(() => {
      globalThis.__calls = []; globalThis.__count = 0;
      globalThis.fabushi = { invoke: async (tool, args) => {
        globalThis.__calls.push({ tool, args });
        if (tool === 'transfer.send') { globalThis.__count = 0; if (globalThis.__deny) return { ok: false, error: { message: 'fixture-send-denied' } }; }
        const data = tool === 'offline.capabilities' ? { platform: 'desktop', capabilities: ['selection', 'transfer', 'config'], features: ['link-download-v1'], targetPools: [{ id: 'configured', label: 'Authorized receiver' }, { id: 'lan', label: 'Authorized LAN' }] }
          : tool === 'selection.compose' ? (args.downloadLinks ? { operationId: 'fixture-download' } : { selectionId: 'fixture-selection', entries: [] })
          : tool === 'transfer.send' ? { operationId: 'fixture-operation' }
          : tool === 'transfer.status' && args.operationId === 'fixture-download' ? { state: 'complete', kind: 'download', result: { selectionId: 'fixture-downloaded', entries: [{ name: 'linked.pdf', size: 99 }], warnings: [] } }
          : tool === 'transfer.status' ? (globalThis.__count++ === 0 || globalThis.__continuous ? { state: 'running', progress: { sequence: globalThis.__count, pass_chunks_sent: 1, pass_verified_payload_bytes: 7 } } : { state: 'complete', result: { verified_payload_bytes: 7, chunks_sent: 1 } }) : { imported: true };
        return { ok: true, data };
      } };
    });
    await page.reload(); await ready(); await page.locator('#mode').selectOption('global');
    await page.waitForFunction(() => document.getElementById('composer-target').value === 'configured');
    await page.locator('#open-settings').click(); await page.locator('#rate-global').fill('7');
    for (const [id, value] of [['origin-lat', '31.23'], ['origin-lon', '121.47'], ['destination-lat', '37.77'], ['destination-lon', '-122.42']]) await page.locator('#' + id).fill(value);
    await page.locator('#set-origin').click(); await page.locator('#import-config').click();
    await page.waitForFunction(() => document.getElementById('settings-status').textContent.includes('已导入'));
    await page.locator('#message').fill('https://example.invalid/linked.pdf');
    await page.locator('#message').dispatchEvent('paste');
    await page.waitForFunction(() => document.getElementById('composer-file-list').textContent.includes('linked.pdf'));
    check(await page.locator('#message').inputValue(), '');
    check(await page.evaluate(() => globalThis.__calls.filter(c => c.tool === 'transfer.send').length), 0);
    check(await page.evaluate(() => globalThis.__calls.some(c => c.tool === 'selection.compose' && c.args.downloadLinks === true)), true);
    await page.locator('#open-settings').click(); await page.locator('#message').fill('authorized global fixture'); await page.locator('#launch').click(); await complete();
    const send = await page.evaluate(() => globalThis.__calls.find(c => c.tool === 'transfer.send').args);
    check(send.rateBytesPerSecond, 7 * 1024); check(send.repeat, 1); check(send.targetPoolId, 'configured');
    check(await page.locator('#earth').getAttribute('data-operation'), 'confirmed');
    await page.evaluate(() => { globalThis.__deny = true; }); await page.locator('#launch').click();
    await page.waitForFunction(() => document.getElementById('composer-status').textContent.includes('fixture-send-denied'));
    check(await page.locator('#earth').getAttribute('data-operation'), 'idle');
    await page.evaluate(() => { globalThis.__deny = false; globalThis.__continuous = true; globalThis.__calls = []; });
    await page.locator('#mode').selectOption('hotspot'); await page.locator('#cycles').check(); await page.locator('#message').fill('LAN fixture'); await page.locator('#launch').click();
    await page.waitForFunction(() => document.getElementById('earth').dataset.operation === 'running');
    const looping = await page.evaluate(() => globalThis.__calls.find(c => c.tool === 'transfer.send').args);
    check(looping.watch, true); check(looping.targetPoolId, 'lan'); check(Object.hasOwn(looping, 'repeat'), false);
    check(looping.rateBytesPerSecond, 1024 * 1024);
    await page.locator('#stop-composer').click(); check(await page.evaluate(() => globalThis.__calls.some(c => c.tool === 'transfer.cancel')), true);
    check(external, []); check(errors, []);
    await page.screenshot({ path: 'miniapp-ui-native-narrow.png', fullPage: true });
    console.log(JSON.stringify({ tests: count, source: process.env.TESTED_COMMIT, result: 'pass', scope: 'real-Wasm-WebGL-and-native-UI-fixture-not-installed-acceptance' }));
  } finally { await context.close(); await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
