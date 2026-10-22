// Fabushi Mini App host tool contract v1.
//
// One web bundle, three hosts. The page never receives filesystem paths,
// credentials or shell access: it receives opaque handles issued by the host
// after a user gesture, and every call is validated host-side.
//
// Bridge shapes, in preference order:
//   ios/android  window.__fabushiWebMcp.call(name, input) -> Promise<result>
//                (both native hosts inject this; iOS reports version 2, Android 1)
//   android      window.FabushiWebMcpNative.callTool(name, json) -> json string
//   desktop      window.fabushi.invoke(tool, args) -> Promise<{ok,data}>
// A host that grants no tools simply reports none; the page stays offline-only.
export const TOOL_CONTRACT = 'fabushi.miniapp.tools.v1';

export const CAPABILITIES = Object.freeze({
  OFFLINE: 'offline',
  SELECTION: 'selection',
  TRANSFER: 'transfer',
  SCAN: 'scan',
  STATE: 'state',
  CONFIG: 'config',
  PLATFORM: 'platform',
  HOTSPOT: 'hotspot',
  UPDATE: 'update',
  DIAGNOSTICS: 'diagnostics',
  COMPAT: 'compat',
});

// Declarative surface. `mutating` drives the host approval prompt.
export const TOOLS = Object.freeze({
  'offline.capabilities': { capability: CAPABILITIES.OFFLINE, mutating: false },
  'offline.state.read': { capability: CAPABILITIES.OFFLINE, mutating: false },
  'selection.pick': { capability: CAPABILITIES.SELECTION, mutating: false },
  'selection.compose': { capability: CAPABILITIES.SELECTION, mutating: false },
  'selection.release': { capability: CAPABILITIES.SELECTION, mutating: false },
  'transfer.send': { capability: CAPABILITIES.TRANSFER, mutating: true },
  'transfer.receive': { capability: CAPABILITIES.TRANSFER, mutating: true },
  'transfer.configureReceiver': { capability: CAPABILITIES.TRANSFER, mutating: true },
  'transfer.status': { capability: CAPABILITIES.TRANSFER, mutating: false },
  'transfer.cancel': { capability: CAPABILITIES.TRANSFER, mutating: true },
  'scan.metadata': { capability: CAPABILITIES.SCAN, mutating: false },
  'scan.read': { capability: CAPABILITIES.SCAN, mutating: false },
  'state.inspect': { capability: CAPABILITIES.STATE, mutating: false },
  'state.import': { capability: CAPABILITIES.STATE, mutating: true },
  'config.defaults': { capability: CAPABILITIES.CONFIG, mutating: false },
  'config.check': { capability: CAPABILITIES.CONFIG, mutating: false },
  'config.import': { capability: CAPABILITIES.CONFIG, mutating: true },
  'platform.inspect': { capability: CAPABILITIES.PLATFORM, mutating: false },
  'platform.mounts': { capability: CAPABILITIES.PLATFORM, mutating: false },
  'platform.time': { capability: CAPABILITIES.PLATFORM, mutating: false },
  'platform.watch': { capability: CAPABILITIES.PLATFORM, mutating: false },
  'hotspot.status': { capability: CAPABILITIES.HOTSPOT, mutating: false },
  'hotspot.plan': { capability: CAPABILITIES.HOTSPOT, mutating: false },
  'hotspot.apply': { capability: CAPABILITIES.HOTSPOT, mutating: true },
  'hotspot.stop': { capability: CAPABILITIES.HOTSPOT, mutating: true },
  'update.check': { capability: CAPABILITIES.UPDATE, mutating: false },
  'update.stage': { capability: CAPABILITIES.UPDATE, mutating: true },
  'diagnostics.selfTest': { capability: CAPABILITIES.DIAGNOSTICS, mutating: false },
  'diagnostics.doctor': { capability: CAPABILITIES.DIAGNOSTICS, mutating: false },
  'diagnostics.status': { capability: CAPABILITIES.DIAGNOSTICS, mutating: false },
  'compat.packetInspect': { capability: CAPABILITIES.COMPAT, mutating: false },
  'compat.countryLookup': { capability: CAPABILITIES.COMPAT, mutating: false },
  'compat.linkExercise': { capability: CAPABILITIES.COMPAT, mutating: true },
  'compat.linkRecover': { capability: CAPABILITIES.COMPAT, mutating: true },
});

const UNAVAILABLE = Object.freeze({
  available: false,
  platform: 'unknown',
  capabilities: new Set(),
  // Resolves rather than throws: a missing host is a normal state, not a fault.
  call: async () => ({
    ok: false,
    error: { code: 'HOST_TOOLS_UNAVAILABLE', message: '当前宿主没有提供能力桥' },
  }),
});

function normalise(result) {
  if (result == null) return { ok: false, error: { code: 'EMPTY_RESULT', message: '宿主未返回结果' } };
  if (typeof result === 'string') {
    try { return normalise(JSON.parse(result)); }
    catch { return { ok: false, error: { code: 'INVALID_RESULT', message: '宿主返回了非 JSON 结果' } }; }
  }
  if (typeof result !== 'object') return { ok: false, error: { code: 'INVALID_RESULT', message: '宿主返回了非对象结果' } };
  if (result.ok === true) {
    // Desktop answers {ok,data}; the injected mobile bridges answer {ok,result}.
    return Object.hasOwn(result, 'data') ? result : { ...result, data: result.result };
  }
  const error = result.error;
  if (typeof error === 'string' && error) return { ok: false, error: { code: 'HOST_ERROR', message: error } };
  return { ok: false, error: error ?? { code: 'HOST_ERROR', message: '宿主拒绝了该操作' } };
}

function detectBridge() {
  const injected = globalThis.__fabushiWebMcp;
  if (injected && typeof injected.call === 'function') {
    return {
      platform: Number(injected.version) >= 2 ? 'ios' : 'android',
      // Injected hosts resolve to the bare result and reject on failure, so a
      // resolved value is success unless it explicitly carries an `ok` flag.
      invoke: async (tool, args) => {
        const value = await injected.call(tool, args ?? {});
        return value && typeof value === 'object' && typeof value.ok === 'boolean'
          ? value
          : { ok: true, data: value };
      },
    };
  }
  const desktop = globalThis.fabushi;
  if (desktop && typeof desktop.invoke === 'function') {
    return {
      platform: 'desktop',
      invoke: (tool, args) => desktop.invoke(tool, args ?? {}),
    };
  }
  const android = globalThis.FabushiWebMcpNative;
  if (android && typeof android.callTool === 'function') {
    return {
      platform: 'android',
      // The Kotlin @JavascriptInterface takes two positional arguments.
      invoke: async (tool, args) => android.callTool(tool, JSON.stringify(args ?? {})),
    };
  }
  return null;
}

/**
 * Resolve the host bridge and ask it what it can actually do.
 * Never throws: an unavailable host degrades to the offline workbench.
 */
export async function connectHost() {
  const bridge = detectBridge();
  if (!bridge) return { ...UNAVAILABLE, reason: 'NO_HOST_BRIDGE' };
  const capabilities = new Set();
  const call = async (tool, args = {}) => {
    if (!Object.hasOwn(TOOLS, tool)) {
      return { ok: false, error: { code: 'UNKNOWN_TOOL', message: `未声明的工具：${tool}` } };
    }
    try {
      const result = normalise(await bridge.invoke(tool, args));
      if (tool === 'offline.capabilities' && result.ok && Array.isArray(result.data?.capabilities)) {
        capabilities.clear();
        for (const capability of result.data.capabilities) if (typeof capability === 'string') capabilities.add(capability);
      }
      return result;
    }
    catch (error) {
      const message = String(error?.message ?? error);
      // Both mobile hosts reject ungranted tools with this exact prefix.
      const code = /capability not granted/i.test(message) ? 'CAPABILITY_NOT_GRANTED'
        : /Unknown WebMCP tool/i.test(message) ? 'TOOL_NOT_DECLARED'
          : 'TRANSPORT_ERROR';
      return { ok: false, error: { code, message } };
    }
  };
  const announced = await call('offline.capabilities', { contract: TOOL_CONTRACT });
  return {
    available: announced.ok,
    platform: announced.ok ? String(announced.data?.platform ?? bridge.platform) : bridge.platform,
    capabilities,
    contract: announced.ok ? String(announced.data?.contract ?? '') : '',
    call,
    reason: announced.ok ? null : announced.error?.code ?? 'CAPABILITIES_UNAVAILABLE',
  };
}

/**
 * Best-effort page-tool registration for hosts that drive the page (iOS/Android).
 * Host-invoked tools stay read-only over page state; they never reach native.
 */
export function registerPageTools(tools) {
  const injected = globalThis.__fabushiWebMcp;
  if (injected && typeof injected.registerLocalTool === 'function') {
    for (const tool of tools) {
      try {
        injected.registerLocalTool({
          name: tool.name,
          description: tool.description,
          inputSchema: { type: 'object', properties: {} },
          annotations: { readOnlyHint: true },
        }, tool.run);
      } catch { /* the host decides which page tools it accepts */ }
    }
    return 'injected';
  }
  const modelContext = globalThis.document?.modelContext;
  if (modelContext && typeof modelContext.registerTool === 'function') {
    for (const tool of tools) {
      try {
        modelContext.registerTool({ name: tool.name, description: tool.description, inputSchema: { type: 'object', properties: {} } }, tool.run);
      } catch { /* host rejects duplicates; not fatal */ }
    }
    return 'modelContext';
  }
  if (globalThis.parent && globalThis.parent !== globalThis) {
    try {
      globalThis.parent.postMessage({ jsonrpc: '2.0', method: 'ready', params: { tools: tools.map(({ name, description }) => ({ name, description, inputSchema: { type: 'object', properties: {} } })) } }, '*');
      return 'postMessage';
    } catch { /* no bridge */ }
  }
  return null;
}

export function formatBytes(size) {
  if (!Number.isFinite(size) || size < 0) return '—';
  if (size < 1024) return `${size} B`;
  if (size < 1024 ** 2) return `${(size / 1024).toFixed(1)} KiB`;
  if (size < 1024 ** 3) return `${(size / 1024 ** 2).toFixed(1)} MiB`;
  return `${(size / 1024 ** 3).toFixed(2)} GiB`;
}
