const { chromium } = require('playwright');
const { readFile } = require('node:fs/promises');
const { resolve, sep } = require('node:path');
const assert = require('node:assert/strict');
const resources = resolve(process.argv[2]);
const origin = 'https://global-dandelion.fixture.invalid';
const mime = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.wasm': 'application/wasm' };
const fs = require('node:fs');
const browser = awaitBrowser();
async function awaitBrowser() {
  const instance = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader'] });
  let count = 0;
  const context = await instance.newContext({ acceptDownloads: true });
  const external = [];
  await context.route('**/*', async route => {
    const url = new URL(route.request().url());
    if (url.origin !== origin) { external.push(url.origin); return route.abort(); }
    const path = resolve(resources, '.' + decodeURIComponent(url.pathname));
    if (!path.startsWith(resources + sep)) return route.abort();
    try { await route.fulfill({ body: await readFile(path), contentType: mime[require('node:path').extname(path)] || 'application/octet-stream' }); }
    catch { await route.fulfill({ status: 404, body: '' }); }
  });
  await context.setOffline(true);
  const page = await context.newPage();
  try {
    await page.goto(origin + '/index.html');
    await page.waitForFunction(() => document.getElementById('status').textContent.includes('已就绪'));
    await page.waitForFunction(() => document.getElementById('composer-status').textContent.includes('点击 ↑'));
    await page.locator('#message').fill('愿一切安好'); await page.locator('#cycles').selectOption('3');
    await page.locator('#launch').click();
    await page.waitForFunction(() => document.getElementById('composer-status').textContent.includes('本地回环完成'));
    assert.equal(await page.locator('#composer-pass').textContent(), '3 / 3'); count++;
    const sessionDownloadPromise = page.waitForEvent('download'); await page.locator('#composer-export').click();
    const sessionDownload = await sessionDownloadPromise;
    const session = JSON.parse(fs.readFileSync(await sessionDownload.path(), 'utf8'));
    assert.equal(session.verifiedBytes, Buffer.byteLength('愿一切安好') * 3); assert.equal(session.networkSent, false); count++;
    await page.locator('.route-settings > summary').click();
    await page.locator('#origin-lat').fill('31.23'); await page.locator('#origin-lon').fill('121.47'); await page.locator('#set-origin').click();
    assert.match(await page.locator('#location-summary').textContent(), /31.230/); count++;
    await page.locator('.route-settings > summary').click();
    await page.locator('#composer-files').setInputFiles({ name: 'ripple.txt', mimeType: 'text/plain', buffer: Buffer.alloc(1024 * 1024) });
    await page.locator('#cycles').selectOption('100'); await page.locator('#launch').click();
    await page.waitForFunction(() => document.getElementById('earth').dataset.operation === 'running');
    await page.screenshot({ path: 'miniapp-ui-globe-running.png', fullPage: true });
    await page.locator('#stop-composer').click();
    assert.match(await page.locator('#composer-status').textContent(), /已停止/); assert.equal(await page.locator('#composer-export').isDisabled(), true); count++;
    await page.locator('#mode').selectOption('global'); await page.locator('#launch').click();
    await page.waitForFunction(() => document.getElementById('composer-status').textContent.includes('尚未安装'));
    assert.notEqual(await page.locator('#earth').getAttribute('data-operation'), 'running'); count++;
    await page.locator('#mode').selectOption('local'); await page.locator('#cycles').selectOption('1');
    await page.setViewportSize({ width: 390, height: 844 });
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.locator('#message').fill('mobile Rust'); await page.locator('#launch').click();
    await page.waitForFunction(() => document.getElementById('composer-status').textContent.includes('本地回环完成'));
    await page.screenshot({ path: 'miniapp-ui-globe-mobile.png', fullPage: true });
    const overflow = await page.evaluate(() => [...document.querySelectorAll('body *')].filter(node => node.getBoundingClientRect().right > innerWidth + 1).map(node => ({ tag: node.tagName, id: node.id, class: node.className, right: node.getBoundingClientRect().right })).slice(0, 12));
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true, JSON.stringify(overflow)); count++;
    const pixels = await page.evaluate(() => { const gl = document.getElementById('earth').getContext('webgl'); if (!gl) return 0; const pixel = new Uint8Array(4); gl.readPixels(gl.drawingBufferWidth / 2, gl.drawingBufferHeight / 2, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel); return pixel[3]; });
    assert.ok(pixels > 0, 'real WebGL sphere must render'); count++;
    await page.screenshot({ path: 'miniapp-ui-globe-mobile.png', fullPage: true });
    await page.setViewportSize({ width: 1180, height: 1000 });
    await page.screenshot({ path: 'miniapp-ui-globe-desktop.png', fullPage: true });
    await page.locator('#advanced-workbench > summary').click();
    assert.equal(await page.locator('#start').isDisabled(), true); count++;
    await page.locator('#files').setInputFiles([{ name: '<img onerror=alert(1)>.txt', mimeType: 'text/plain', buffer: Buffer.from('hello') }, { name: 'empty.txt', mimeType: 'text/plain', buffer: Buffer.alloc(0) }]);
    assert.equal(await page.locator('#selection img').count(), 0); count++;
    await page.locator('#start').click();
    await page.waitForFunction(() => document.getElementById('status').textContent.includes('本地校验完成'));
    assert.equal(await page.locator('#file-count').textContent(), '2');
    assert.equal(await page.locator('#packet-count').textContent(), '2'); count++;
    const downloadPromise = page.waitForEvent('download'); await page.locator('#export').click();
    const download = await downloadPromise;
    const report = JSON.parse(fs.readFileSync(await download.path(), 'utf8'));
    assert.equal(report.networkSent, false); assert.equal(report.verifiedBytes, 5); assert.equal(report.status, 'complete'); count++;
    const negative = await page.evaluate(async () => {
      const codec = await import('./pkg/dandelion_web_adapter.js');
      const frame = codec.encode_packet(new Uint8Array([1, 2, 3]), new Uint8Array(16), 3, 0, true);
      let rejects = 0;
      for (const bad of [frame.slice(0, 20), new Uint8Array(2000), new Uint8Array([...frame, 0])]) {
        try { codec.verify_packet(bad); } catch { rejects++; }
      }
      frame[80] ^= 1; try { codec.verify_packet(frame); } catch { rejects++; }
      return rejects;
    });
    assert.equal(negative, 4); count++;
    await page.locator('#files').setInputFiles({ name: 'large.bin', mimeType: 'application/octet-stream', buffer: Buffer.alloc(1024 * 1024) });
    await page.locator('#start').click(); await page.locator('#cancel').click();
    await page.waitForFunction(() => document.getElementById('status').textContent.includes('已取消'));
    assert.equal(await page.locator('#export').isDisabled(), true); count++;
    await page.locator('#files').setInputFiles({ name: 'next.txt', mimeType: 'text/plain', buffer: Buffer.from('next') });
    await page.locator('#start').click();
    await page.waitForFunction(() => document.getElementById('status').textContent.includes('本地校验完成')); count++;
    await page.locator('details.card > summary').click();
    await page.locator('#remember').check(); await page.locator('#clear').click();
    assert.equal(await page.locator('#remember').isChecked(), false); count++;
    await page.reload();
    await page.waitForFunction(() => document.getElementById('status').textContent.includes('已就绪'));
    assert.equal(await page.locator('#selection li').count(), 0); count++;
    await page.locator('#advanced-workbench > summary').click();
    assert.deepEqual(external, []); count++;
    await page.locator('#files').setInputFiles(Array.from({ length: 129 }, (_, i) => ({ name: `file-${i}.txt`, mimeType: 'text/plain', buffer: Buffer.from('x') })));
    assert.equal(await page.locator('#start').isDisabled(), true);
    assert.match(await page.locator('#selection-summary').textContent(), /超出限制/); count++;
    await page.screenshot({ path: 'miniapp-ui.png', fullPage: true });
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.screenshot({ path: 'miniapp-ui-dark.png', fullPage: true });
    await page.setViewportSize({ width: 390, height: 844 });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true); count++;
    await page.screenshot({ path: 'miniapp-ui-narrow.png', fullPage: true });
    // UI fixture only: installed native execution is verified in Fabushi Host CI.
    await context.addInitScript(() => {
      globalThis.__fixtureCalls = [];
      globalThis.fabushi = { invoke: async (tool, args) => {
        globalThis.__fixtureCalls.push({ tool, args });
        const data = tool === 'offline.capabilities'
          ? { contract: 'fabushi.miniapp.tools.v1', platform: 'desktop', capabilities: ['offline', 'selection', 'scan', 'transfer', 'config', 'state', 'compat'], targetPools: [{ id: 'configured', label: 'Authorized fixture' }, { id: 'lan', label: 'LAN fixture' }] }
          : tool === 'selection.pick' ? { selectionId: 'fixture-root', entries: [{ name: 'fixture', size: 7 }] }
          : tool === 'scan.metadata' ? { regular_files: 2, total_file_bytes: 7, payload_bytes_read: 0 }
          : tool === 'scan.read' ? { files_completed: 2, verified_payload_bytes: 7 }
          : tool === 'selection.compose' ? { selectionId: 'fixture-composed', entries: [{ name: 'text', size: 7 }] }
          : tool === 'transfer.send' ? { operationId: 'fixture-send' }
          : tool === 'transfer.receive' ? { operationId: 'fixture-receive' }
          : tool === 'transfer.status' ? { state: 'complete', result: { verified_payload_bytes: 7 } }
          : { imported: true };
        return { ok: true, data };
      } };
    });
    await page.reload();
    await page.waitForFunction(() => document.getElementById('composer-status').textContent.includes('点击 ↑'));
    await page.locator('#mode').selectOption('hotspot');
    await page.waitForFunction(() => document.getElementById('composer-target').value === 'lan');
    await page.locator('#message').fill('LAN fixture'); await page.locator('#launch').click();
    await page.waitForFunction(() => document.getElementById('composer-status').textContent.includes('引擎已完成'));
    const invocation = await page.evaluate(() => globalThis.__fixtureCalls.find(c => c.tool === 'transfer.send'));
    assert.equal(invocation.args.targetPoolId, 'lan'); assert.equal(invocation.args.selectionId, 'fixture-composed'); count++;
    assert.equal(await page.locator('#composer-bytes').textContent(), '7 B'); count++;
    await page.locator('#advanced-workbench > summary').click();
    await page.locator('#pick-root').waitFor({ state: 'visible' });
    await page.locator('#pick-root').click();
    await page.locator('#scan-metadata').click();
    await page.waitForFunction(() => document.getElementById('scan-status').textContent.includes('2 个文件 · 7 B')); count++;
    await page.locator('#scan-read').click();
    await page.waitForFunction(() => document.getElementById('scan-status').textContent.includes('2 个文件 · 7 B')); count++;
    await page.locator('#receive').click();
    await page.waitForFunction(() => document.getElementById('transfer-status').textContent.includes('接收完成，已校验 7 B')); count++;
    assert.equal(await page.locator('#send').isEnabled(), true); count++;
    await page.locator('[data-native-tool="state.import"]').click();
    await page.waitForFunction(() => document.getElementById('advanced-status').textContent.includes('imported')); count++;
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true); count++;
    await page.screenshot({ path: 'miniapp-ui-native-narrow.png', fullPage: true });
    console.log(JSON.stringify({ tests: count, source: process.env.TESTED_COMMIT, result: 'pass', scope: 'offline-and-native-ui-fixture-not-Fabushi-installed-acceptance' }));
  } finally { await context.close(); await instance.close(); }
}
browser.catch(error => { console.error(error); process.exitCode = 1; });
