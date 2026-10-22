// Controlled download fixture -> native staging -> actual Rust loopback bytes.
const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const { mkdtemp, writeFile, readdir, rm } = require('node:fs/promises');
const { tmpdir } = require('node:os');
const path = require('node:path');
const { createDandelionCliBackend } = require(path.resolve(process.argv[2]));
(async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'dandelion-link-contract-'));
  const binary = path.resolve(process.argv[3]), config = path.resolve(process.argv[4]);
  let slow = false, downloadCalls = 0;
  const backend = createDandelionCliBackend({ binaryPath: binary, configPath: config, allowedRoots: [], authorizePickedRoots: true, stateDirectory: root, targetPools: [{ id: 'loopback', label: 'Local' }] }, {
    pick: async () => [{ path: path.join(root, 'picked.txt'), name: 'picked.txt', size: 3 }],
    spawn: argv => {
      if (argv[0] === 'download') {
        downloadCalls++;
        let stop;
        const directory = argv[argv.indexOf('--directory') + 1];
        const done = slow ? new Promise(resolve => { stop = () => resolve({ code: 130, stdout: '', stderr: '' }); }) : (async () => {
          await writeFile(path.join(directory, 'resource-001.bin'), 'downloaded content');
          return { code: 0, stdout: JSON.stringify({ schema_version: 1, entries: [{ name: 'document.pdf', file: 'resource-001.bin', size: 18 }], warnings: [] }) + '\n', stderr: '' };
        })();
        return { done, kill: () => stop?.() };
      }
      const child = spawn(binary, ['--config', config, ...argv], { stdio: ['ignore', 'pipe', 'pipe'] });
      let stdout = '', stderr = '';
      child.stdout.on('data', b => { stdout += b; }); child.stderr.on('data', b => { stderr += b; });
      return { done: new Promise((resolve, reject) => { child.once('error', reject); child.once('close', code => resolve({ code, stdout, stderr })); }), kill: () => child.kill('SIGTERM') };
    },
  });
  const complete = async id => {
    for (let i = 0; i < 100; i++) { const s = await backend.status(id); if (s.state !== 'running') return s; await new Promise(r => setTimeout(r, 50)); }
    throw new Error('operation timeout');
  };
  try {
    await writeFile(path.join(root, 'picked.txt'), 'old');
    const picked = await backend.pick('files', { maxFiles: 128, maxBytes: 1024 ** 3 });
    const job = await backend.downloadLinks('https://example.invalid/document.pdf', picked.selectionId);
    const ready = await complete(job.operationId); assert.equal(ready.state, 'complete');
    assert.equal(ready.result.entries[1].name, 'document.pdf');
    const sent = await backend.send({ selectionId: ready.result.selectionId, targetPoolId: 'loopback' });
    const receipt = await complete(sent.operationId); assert.equal(receipt.state, 'complete');
    assert.equal(receipt.result.verified_payload_bytes, 21); // old + downloaded bytes, not URL text
    assert.equal(downloadCalls, 1);
    slow = true;
    const cancelled = await backend.downloadLinks('https://example.invalid/slow');
    await backend.release(cancelled.operationId);
    assert.equal((await complete(cancelled.operationId)).state, 'cancelled');
    await backend.cancelAll();
    assert.deepEqual((await readdir(root)).filter(v => v.startsWith('download-') || v.startsWith('selection-')), []);
    console.log(JSON.stringify({ actualRustLoopbackBytes: 21, preservedOriginal: true, cancelCleanup: true }));
  } finally { await backend.cancelAll(); await rm(root, { recursive: true, force: true }); }
})().catch(error => { console.error(error); process.exitCode = 1; });
