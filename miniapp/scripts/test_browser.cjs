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
  const instance = await chromium.launch();
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
    await page.locator('summary').click();
    await page.locator('#remember').check(); await page.locator('#clear').click();
    assert.equal(await page.locator('#remember').isChecked(), false); count++;
    await page.reload();
    await page.waitForFunction(() => document.getElementById('status').textContent.includes('已就绪'));
    assert.equal(await page.locator('#selection li').count(), 0); count++;
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
    console.log(JSON.stringify({ tests: count, source: process.env.TESTED_COMMIT, result: 'pass', scope: 'offline-browser-fixture-not-Fabushi-installed-acceptance' }));
  } finally { await context.close(); await instance.close(); }
}
browser.catch(error => { console.error(error); process.exitCode = 1; });
