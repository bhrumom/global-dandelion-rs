// Actions-only protocol/real-engine fixture. Not an installed Host/picker claim.
const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const { mkdtemp, readFile, writeFile, rm } = require('node:fs/promises');
const { tmpdir } = require('node:os');
const path = require('node:path');
const { randomUUID } = require('node:crypto');

(async () => {
  assert.equal(process.env.GITHUB_ACTIONS, 'true');
  const fixture = await mkdtemp(path.join(tmpdir(), 'dandelion-owned-worker-'));
  await writeFile(path.join(fixture, 'payload.txt'), 'actual native payload');
  const privateData = await mkdtemp(path.join(tmpdir(), 'dandelion-private-profile-'));
  const importFile = path.join(privateData, 'import.toml');
  await writeFile(importFile, '[scanner]\nroots=["/tmp/imported-root"]\nallowed_mount_prefixes=["/tmp/imported-mount"]\n[state]\nenabled=true\ndirectory="/tmp/unscoped-shared-state"\n');
  const worker = spawn(process.execPath, [path.resolve(process.argv[2], 'worker.cjs')], { env: { ...process.env, FABUSHI_NATIVE_DATA_DIRECTORY: privateData }, stdio: ['pipe', 'pipe', 'inherit'] });
  const pending = new Map();
  let buffer = '';
  const scope = { appId: 'org.fabushi.global-dandelion', version: '0.4.0', packageDigest: 'a'.repeat(64), accountScope: 'b'.repeat(64), accountGeneration: 1 };
  const send = value => worker.stdin.write(JSON.stringify(value) + '\n');
  worker.stdout.setEncoding('utf8');
  worker.stdout.on('data', chunk => {
    buffer += chunk;
    assert.ok(Buffer.byteLength(buffer) <= 1024 * 1024);
    let end;
    while ((end = buffer.indexOf('\n')) >= 0) {
      const message = JSON.parse(buffer.slice(0, end)); buffer = buffer.slice(end + 1);
      if (message.kind === 'host') {
        assert.ok(pending.has(message.id));
        if (message.method === 'confirmation') send({ kind: 'host-result', callId: message.callId, result: true });
        else {
          assert.equal(message.method, 'file-picker');
          assert.ok(['directory', 'files'].includes(message.mode));
          send({ kind: 'host-result', callId: message.callId, result: [{ path: message.mode === 'directory' ? fixture : importFile, name: 'fixture', size: 0 }] });
        }
      } else { assert.equal(message.kind, 'result'); const resolve = pending.get(message.id); assert.ok(resolve); pending.delete(message.id); resolve(message.result); }
    }
  });
  const call = (method, args = {}) => new Promise((resolve, reject) => {
    const id = randomUUID();
    const timeout = setTimeout(() => reject(new Error('worker response timeout: ' + method)), 30000);
    pending.set(id, result => { clearTimeout(timeout); resolve(result); });
    send({ kind: 'invoke', id, scope, method, args });
  });
  try {
    const capabilities = await call('offline.capabilities');
    assert.equal(capabilities.ok, true);
    for (const c of ['selection', 'scan', 'transfer', 'state', 'config', 'platform', 'diagnostics']) assert.ok(capabilities.data.capabilities.includes(c));
    assert.ok(!capabilities.data.capabilities.includes('hotspot'));
    const selftest = await call('diagnostics.selfTest');
    assert.equal(selftest.ok, true); assert.equal(selftest.data.verified_payload_bytes, 65537);
    const selected = await call('selection.pick', { mode: 'directory' }); assert.equal(selected.ok, true);
    const scan = await call('scan.read', { rootId: selected.data.selectionId });
    assert.equal(scan.ok, true); assert.equal(scan.data.verified_payload_bytes, Buffer.byteLength('actual native payload'));
    const sent = await call('transfer.send', { selectionId: selected.data.selectionId, targetPoolId: 'loopback' });
    assert.equal(sent.ok, true);
    let status;
    for (let i = 0; i < 20; i++) {
      status = await call('transfer.status', { operationId: sent.data.operationId });
      if (status.data.state !== 'running') break;
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    assert.equal(status.ok, true); assert.equal(status.data.state, 'complete');
    assert.equal((await call('scan.read', { rootId: '/etc' })).ok, false);
    assert.equal((await call('selection.release', { selectionId: selected.data.selectionId })).ok, true);
    assert.equal((await call('scan.read', { rootId: selected.data.selectionId })).ok, false);
    const time = await call('platform.time'); assert.equal(time.ok, true); assert.equal(time.data.system_clock_modified, false);
    const mounts = await call('platform.mounts'); assert.equal(mounts.ok, true); assert.equal(mounts.data.observationOnly, true);
    assert.equal((await call('config.import')).ok, true);
    const configured = await call('config.check'); assert.equal(configured.ok, true);
    assert.deepEqual(configured.data.scanner.roots, { count: 0 });
    assert.equal(configured.data.state.enabled, true);
    assert.ok(!JSON.stringify(configured).includes(privateData));
    const stored = await readFile(path.join(privateData, 'service.toml'), 'utf8');
    assert.ok(stored.includes(path.join(privateData, 'state')));
    assert.ok(!stored.includes('/tmp/unscoped-shared-state'));
    await writeFile(importFile, 'invalid [');
    assert.equal((await call('config.import')).ok, false);
    assert.equal(await readFile(path.join(privateData, 'service.toml'), 'utf8'), stored);
    assert.equal((await call('state.inspect')).ok, true);
    console.log('package-owned native worker: actual engine, scan, loopback, release and invalid-ID rejection passed');
  } finally {
    worker.kill('SIGTERM');
    await new Promise(resolve => worker.once('exit', resolve));
    await rm(fixture, { recursive: true, force: true });
    await rm(privateData, { recursive: true, force: true });
  }
})().catch(error => { console.error(error); process.exit(1); });
