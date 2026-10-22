// Contract tests for the shared host-tool layer. No native host is required:
// each test installs the exact bridge shape a real host injects.
import assert from 'node:assert/strict';
import test from 'node:test';

const { connectHost, registerPageTools, TOOLS, TOOL_CONTRACT } = await import('../web/host-tools.js');

const CAPABILITY_ANSWER = Object.freeze({
  contract: TOOL_CONTRACT,
  platform: 'test',
  capabilities: ['offline', 'selection', 'transfer', 'scan', 'hotspot'],
  targetPools: [{ id: 'pool-1', label: '局域网目标' }],
});
// A host that does not self-report a platform keeps the bridge's own label.
const UNLABELLED_ANSWER = Object.freeze({ ...CAPABILITY_ANSWER, platform: undefined });

function clearGlobals() {
  delete globalThis.fabushi;
  delete globalThis.FabushiWebMcpNative;
  delete globalThis.__fabushiWebMcp;
  delete globalThis.document;
}

test('an absent bridge degrades to offline only', async () => {
  clearGlobals();
  const host = await connectHost();
  assert.equal(host.available, false);
  assert.equal(host.reason, 'NO_HOST_BRIDGE');
  assert.equal(host.capabilities.size, 0);
  const result = await host.call('selection.pick', {});
  assert.equal(result.ok, false);
  assert.equal(result.error.code, 'HOST_TOOLS_UNAVAILABLE');
});

test('desktop answers {ok,data} and is recognised', async () => {
  clearGlobals();
  const seen = [];
  globalThis.fabushi = {
    invoke: async (tool, args) => {
      seen.push([tool, args]);
      return { ok: true, data: CAPABILITY_ANSWER };
    },
  };
  const host = await connectHost();
  assert.equal(host.available, true);
  assert.equal(host.platform, 'test');
  assert.deepEqual([...host.capabilities].sort(), ['hotspot', 'offline', 'scan', 'selection', 'transfer']);
  assert.deepEqual(seen[0], ['offline.capabilities', { contract: TOOL_CONTRACT }]);
});

test('ios injected bridge resolves bare results and reports version 2', async () => {
  clearGlobals();
  globalThis.__fabushiWebMcp = {
    version: 2,
    call: async (name) => {
      if (name === 'offline.capabilities') return UNLABELLED_ANSWER;
      throw new Error('Unknown WebMCP tool: ' + name);
    },
  };
  const host = await connectHost();
  assert.equal(host.available, true);
  assert.equal(host.platform, 'ios');
  assert.equal(host.capabilities.has('transfer'), true);
  const missing = await host.call('update.check', {});
  assert.equal(missing.ok, false);
  assert.equal(missing.error.code, 'TOOL_NOT_DECLARED');
});

test('android injected bridge reports version 1 and rejects ungranted tools', async () => {
  clearGlobals();
  globalThis.__fabushiWebMcp = {
    version: 1,
    call: async () => { throw new Error('MCP App bridge capability not granted: offline.capabilities'); },
  };
  const host = await connectHost();
  assert.equal(host.available, false);
  assert.equal(host.platform, 'android');
  assert.equal(host.reason, 'CAPABILITY_NOT_GRANTED');
  assert.equal(host.capabilities.size, 0);
});

test('legacy android interface is called with two positional arguments', async () => {
  clearGlobals();
  const calls = [];
  globalThis.FabushiWebMcpNative = {
    callTool: (name, argumentsJson) => {
      calls.push([name, argumentsJson]);
      return JSON.stringify({ ok: true, result: UNLABELLED_ANSWER });
    },
  };
  const host = await connectHost();
  assert.equal(host.available, true);
  assert.equal(host.platform, 'android');
  assert.equal(calls.length, 1);
  assert.equal(calls[0][0], 'offline.capabilities');
  assert.equal(typeof calls[0][1], 'string');
  assert.deepEqual(JSON.parse(calls[0][1]), { contract: TOOL_CONTRACT });
});

test('string and object host errors normalise to a code and a message', async () => {
  clearGlobals();
  globalThis.FabushiWebMcpNative = {
    callTool: () => JSON.stringify({ ok: false, error: '用户取消了 WebMCP Tool 调用' }),
  };
  const host = await connectHost();
  assert.equal(host.available, false);
  assert.equal(host.reason, 'HOST_ERROR');
  assert.equal(host.capabilities.size, 0);
});

test('undeclared tools never reach the host', async () => {
  clearGlobals();
  let called = 0;
  globalThis.fabushi = { invoke: async () => { called++; return { ok: true, data: {} }; } };
  const host = await connectHost();
  const result = await host.call('fs.read', {});
  assert.equal(result.ok, false);
  assert.equal(result.error.code, 'UNKNOWN_TOOL');
  assert.equal(called, 1); // only the capabilities probe
  assert.equal(Object.hasOwn(TOOLS, 'fs.read'), false);
});

test('page tools register through the injected host when it supports them', () => {
  clearGlobals();
  const registered = [];
  globalThis.__fabushiWebMcp = {
    version: 2,
    call: async () => ({}),
    registerLocalTool: (definition, execute) => { registered.push([definition, execute]); },
  };
  const surface = registerPageTools([{ name: 'offline.state.read', description: 'state', run: () => ({ ready: true }) }]);
  assert.equal(surface, 'injected');
  assert.equal(registered.length, 1);
  assert.equal(registered[0][0].name, 'offline.state.read');
  assert.equal(registered[0][0].annotations.readOnlyHint, true);
  assert.deepEqual(registered[0][1](), { ready: true });
});

test('page tools fall back to modelContext, then to nothing', () => {
  clearGlobals();
  const registered = [];
  globalThis.document = { modelContext: { registerTool: (tool) => registered.push(tool.name) } };
  assert.equal(registerPageTools([{ name: 'a', description: 'a', run: () => {} }]), 'modelContext');
  assert.deepEqual(registered, ['a']);
  clearGlobals();
  assert.equal(registerPageTools([{ name: 'a', description: 'a', run: () => {} }]), null);
});

test('capability refresh updates the existing set after background preparation', async () => {
  clearGlobals();
  let ready = false;
  globalThis.fabushi = { invoke: async () => ({ ok: true, data: { ...CAPABILITY_ANSWER, capabilities: ready ? ['offline', 'selection', 'transfer'] : ['offline'] } }) };
  const host = await connectHost();
  const original = host.capabilities;
  assert.equal(original.has('transfer'), false);
  ready = true;
  await host.call('offline.capabilities');
  assert.equal(host.capabilities, original);
  assert.equal(original.has('transfer'), true);
  ready = false;
  await host.call('offline.capabilities');
  assert.equal(original.has('transfer'), false);
});
