// Actions-only bounded-output and continuous-operation contract.
const assert = require('node:assert/strict');
const { resolve } = require('node:path');
const { mkdtemp, rm } = require('node:fs/promises');
const { tmpdir } = require('node:os');
const { ProgressStream } = require(resolve(process.argv[2]));
const { createDandelionCliBackend } = require(resolve(process.argv[3]));
(async () => {
  const stream = new ProgressStream();
  for (let sequence = 1; sequence <= 50000; sequence++) stream.push(Buffer.from(JSON.stringify({ event: 'progress', sequence, pass_verified_payload_bytes: sequence, pass_chunks_sent: sequence }) + '\n'));
  assert.equal(JSON.parse(stream.snapshot()).sequence, 50000);
  assert.ok(Buffer.byteLength(stream.snapshot()) < 4096);
  stream.push(Buffer.from('{"schema_version":1,"verified_payload_bytes":50000}\n'));
  assert.equal(stream.snapshot().trim().split('\n').length, 2);
  assert.throws(() => stream.push(Buffer.from('{"event":"progress"}\n')));
  assert.throws(() => new ProgressStream().push(Buffer.alloc(1048577, 65)));
  assert.throws(() => new ProgressStream().push(Buffer.from('{"event":"progress","sequence":-1}\n')));
  const partial = new ProgressStream();
  const line = Buffer.from('{"event":"progress","sequence":1,"pass_verified_payload_bytes":1,"pass_chunks_sent":1}\n');
  partial.push(line.subarray(0, 9)); partial.push(line.subarray(9));
  assert.equal(JSON.parse(partial.snapshot()).sequence, 1);
  const root = await mkdtemp(resolve(tmpdir(), 'dandelion-watch-contract-'));
  const calls = [];
  let killed = false, settle;
  const backend = createDandelionCliBackend({ binaryPath: '/fixture/bin', configPath: '/fixture/config', platform: 'linux', allowedRoots: [root], targetPools: [{ id: 'loopback', label: 'Local' }], timeoutMs: 20 }, {
    pick: async () => [{ path: root, name: 'fixture', size: 0 }],
    spawn: argv => { calls.push(argv); return { done: new Promise(resolve => { settle = resolve; }), kill: () => { killed = true; settle({ code: 0, stdout: '{"schema_version":1}\n', stderr: '' }); } }; },
  });
  try {
    const selected = await backend.pick('directory', { maxFiles: 1, maxBytes: 1 });
    const started = await backend.send({ selectionId: selected.selectionId, targetPoolId: 'loopback', watch: true, intervalMs: 100, rateBytesPerSecond: 4096 });
    await new Promise(resolve => setTimeout(resolve, 80));
    assert.equal(killed, false, 'explicit watch must outlive ordinary command timeout');
    assert.equal((await backend.status(started.operationId)).state, 'running');
    assert.ok(calls[0].includes('--watch')); assert.ok(!calls[0].includes('--repeat'));
    assert.equal(calls[0][calls[0].indexOf('--rate-bytes-per-second') + 1], '4096');
    await backend.cancel(started.operationId); assert.equal(killed, true);
    assert.equal((await backend.status(started.operationId)).state, 'cancelled');
  } finally { await backend.cancelAll(); await rm(root, { recursive: true, force: true }); }
  console.log('bounded 50000-frame stream, watch timeout distinction, rate argv and cancellation passed');
})().catch(error => { console.error(error); process.exitCode = 1; });
