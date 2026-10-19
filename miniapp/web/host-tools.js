// Fabushi Mini App host tool contract v1.
//
// One web bundle, three hosts. The page never receives filesystem paths,
// credentials or shell access: it receives opaque handles issued by the host
// after a user gesture, and every call is validated host-side.
//
// Bridge shapes:
//   desktop  window.fabushi.invoke(tool, args)          -> Promise<result>
//   android  window.FabushiWebMcpNative.callTool(json)  -> json string
//   ios      no page -> host channel; page tools are host-invoked only
export const TOOL_CONTRACT = 'fabushi.miniapp.tools.v1';

export const CAPABILITIES = Object.freeze({
  OFFLINE: 'offline',
  SELECTION: 'selection',
  TRANSFER: 'transfer',
  SCAN: 'scan',
  HOTSPOT: 'hotspot',
  UPDATE: 'update',
});

// Declarative surface. `mutating` drives the host approval prompt.
export const TOOLS = Object.freeze({
  'offline.capabilities': { capability: CAPABILITIES.OFFLINE, mutating: false },
  'offline.state.read': { capability: CAPABILITIES.OFFLINE, mutating: false },
  'selection.pick': { capability: CAPABILITIES.SELECTION, mutating: false },
  'selection.release': { capability: CAPABILITIES.SELECTION, mutating: false },
  'transfer.send': { capability: CAPABILITIES.TRANSFER, mutating: true },
  'transfer.status': { capability: CAPABILITIES.TRANSFER, mutating: false },
  'transfer.cancel': { capability: CAPABILITIES.TRANSFER, mutating: true },
  'scan.metadata': { capability: CAPABILITIES.SCAN, mutating: false },
  'scan.read': { capability: CAPABILITIES.SCAN, mutating: false },
  'hotspot.plan': { capability: CAPABILITIES.HOTSPOT, mutating: false },
  'hotspot.apply': { capability: CAPABILITIES.HOTSPOT, mutating: true },
  'hotspot.stop': { capability: CAPABILITIES.HOTSPOT, mutating: true },
  'update.check': { capability: CAPABILITIES.UPDATE, mutating: false },
  'update.stage': { capability: CAPABILITIES.UPDATE, mutating: true },
});

const UNAVAILABLE = Object.freeze({
  available: false,
  platform: 'unknown',
  capabilities: new Set(),
  call: async () => { throw new Error('HOST_TOOLS_UNAVAILABLE'); },
});

function normalise(result) {
  if (result == null) return { ok: false, error: { code: 'EMPTY_RESULT', message: '宿主未返回结果' } };
  if (typeof result === 'string') {
    try { return normalise(JSON.parse(result)); }
    catch { return { ok: false, error: { code: 'INVALID_RESULT', message: '宿主返回了非 JSON 结果' } }; }
  }
  if (typeof result !== 'object') return { ok: false, error: { code: 'INVALID_RESULT', message: '宿主返回了非对象结果' } };
  if (result.ok === true) return result;
  return { ok: false, error: result.error ?? { code: 'HOST_ERROR', message: '宿主拒绝了该操作' } };
}

function detectBridge() {
  const desktop = globalThis.fabushi;
  if (desktop && typeof desktop.invoke === 'function') {
    return {
      platform: 'desktop',
      invoke: (tool, args) => desktop.invoke(tool, args),
    };
  }
  const android = globalThis.FabushiWebMcpNative;
  if (android && typeof android.callTool === 'function') {
    return {
      platform: 'android',
      // The Kotlin bridge is synchronous and takes a single JSON string.
      invoke: async (tool, args) => android.callTool(JSON.stringify({ tool, arguments: args ?? {} })),
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
  const call = async (tool, args = {}) => {
    if (!Object.hasOwn(TOOLS, tool)) {
      return { ok: false, error: { code: 'UNKNOWN_TOOL', message: `未声明的工具：${tool}` } };
    }
    try { return normalise(await bridge.invoke(tool, args)); }
    catch (error) {
      return { ok: false, error: { code: 'TRANSPORT_ERROR', message: String(error?.message ?? error) } };
    }
  };
  const announced = await call('offline.capabilities', { contract: TOOL_CONTRACT });
  const capabilities = new Set(
    announced.ok && Array.isArray(announced.data?.capabilities) ? announced.data.capabilities : [],
  );
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
