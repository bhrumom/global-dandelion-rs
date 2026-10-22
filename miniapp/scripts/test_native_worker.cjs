// Actions-only protocol/real-engine fixture. Not an installed Host/picker claim.
const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const { mkdtemp, readFile, writeFile, rm, readdir, symlink } = require('node:fs/promises');
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
  const worker = spawn(process.execPath, [path.resolve(process.argv[2], 'worker.cjs')], { env: { ...process.env, FABUSHI_NATIVE_DATA_DIRECTORY: privateData, DANDELION_OFFLINE: "1" }, stdio: ['pipe', 'pipe', 'inherit'] });
  const pending = new Map();
  const pickFiles = [];
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
        else if (message.method === 'package-update') send({ kind: 'host-result', callId: message.callId, result: { installedVersion: '0.4.0', availableVersion: '0.4.0', updateAvailable: false } });
        else {
          assert.equal(message.method, 'file-picker');
          assert.ok(['directory', 'files'].includes(message.mode));
          send({ kind: 'host-result', callId: message.callId, result: [{ path: message.mode === 'directory' ? fixture : pickFiles.shift() || importFile, name: 'fixture', size: 0 }] });
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
    assert.equal(capabilities.data.network.state, 'offline');
    assert.equal(capabilities.data.network.receivers, 0);
    for (const c of ['selection', 'scan', 'transfer', 'state', 'config', 'platform', 'diagnostics', 'update', 'compat']) assert.ok(capabilities.data.capabilities.includes(c));
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
    await writeFile(path.join(fixture, 'not-selected.txt'), 'this sibling must never be sent');
    pickFiles.push(path.join(fixture, 'payload.txt'));
    const individual = await call('selection.pick', { mode: 'files' }); assert.equal(individual.ok, true);
    assert.equal(individual.data.entries[0].size, Buffer.byteLength('actual native payload'));
    const exact = await call('scan.read', { rootId: individual.data.selectionId });
    assert.equal(exact.data.files_completed, 1); assert.equal(exact.data.verified_payload_bytes, Buffer.byteLength('actual native payload'));
    const composed = await call('selection.compose', { selectionId: individual.data.selectionId, text: 'hello Rust' }); assert.equal(composed.ok, true);
    const textSend = await call('transfer.send', { selectionId: composed.data.selectionId, targetPoolId: 'loopback', repeat: 2 }); assert.equal(textSend.ok, true);
    await call('selection.release', { selectionId: composed.data.selectionId });
    for (let i = 0; i < 30; i++) { status = await call('transfer.status', { operationId: textSend.data.operationId }); if (status.data.state !== 'running') break; await new Promise(resolve => setTimeout(resolve, 100)); }
    assert.equal(status.data.state, 'complete'); assert.equal(status.data.result.verified_payload_bytes, 2 * (Buffer.byteLength('actual native payload') + Buffer.byteLength('hello Rust')));
    const continuous = await call('transfer.send', { selectionId: individual.data.selectionId, targetPoolId: 'loopback', watch: true, intervalMs: 100, rateBytesPerSecond: 1024 });
    assert.equal(continuous.ok, true);
    await new Promise(resolve => setTimeout(resolve, 1300));
    const watching = await call('transfer.status', { operationId: continuous.data.operationId });
    assert.equal(watching.data.state, 'running');
    assert.ok(watching.data.progress?.sequence >= 1);
    assert.equal((await call('transfer.cancel', { operationId: continuous.data.operationId })).ok, true);
    assert.equal((await call('transfer.status', { operationId: continuous.data.operationId })).data.state, 'cancelled');
    await call('selection.release', { selectionId: individual.data.selectionId });
    await new Promise(resolve => setTimeout(resolve, 100));
    assert.deepEqual((await readdir(path.join(privateData, 'state'))).filter(name => name.startsWith('selection-')), []);
    const alias = path.join(fixture, 'alias.txt'); await symlink(path.join(fixture, 'payload.txt'), alias);
    pickFiles.push(alias); assert.equal((await call('selection.pick', { mode: 'files' })).ok, false);
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
    const update = await call('update.check'); assert.equal(update.ok, true); assert.equal(update.data.updateAvailable, false);
    const packet = path.join(privateData, 'packet.bin');
    await writeFile(packet, Buffer.from('00000007900150983cd24fb0d6963f7d28e17f72616263', 'hex'));
    pickFiles.push(packet);
    const inspected = await call('compat.packetInspect'); assert.equal(inspected.ok, true); assert.equal(inspected.data.index, 7); assert.equal(inspected.data.authenticated, false);
    const locations = path.join(privateData, 'locations.csv'), blocks = path.join(privateData, 'blocks.csv');
    await writeFile(locations, 'geoname_id,country_name\n1,Test Country\n');
    await writeFile(blocks, 'network,geoname_id\n192.0.2.0/24,1\n');
    pickFiles.push(locations, blocks);
    const country = await call('compat.countryLookup', { address: '192.0.2.7' });
    assert.equal(country.ok, true); assert.equal(country.data.country, 'Test Country'); assert.equal(country.data.authorized, false);
    pickFiles.push(path.join(fixture, 'payload.txt'));
    const linked = await call('compat.linkExercise'); assert.equal(linked.ok, true); assert.equal(linked.data.completed_cycles, 10);
    assert.equal((await call('compat.linkRecover')).ok, true);
    assert.equal(await readFile(path.join(fixture, 'payload.txt'), 'utf8'), 'actual native payload');
    console.log('package-owned native worker: actual engine, scan, loopback, release and invalid-ID rejection passed');
  } finally {
    worker.kill('SIGTERM');
    await new Promise(resolve => worker.once('exit', resolve));
    await rm(fixture, { recursive: true, force: true });
    await rm(privateData, { recursive: true, force: true });
  }
})().catch(error => { console.error(error); process.exit(1); });
