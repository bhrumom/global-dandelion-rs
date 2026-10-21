import { connectHost, registerPageTools } from './host-tools.js';
import { mountComposer } from './composer.js';
let rust = null;
try {
  const [adapter, { wasmBytes }] = await Promise.all([import('./pkg/dandelion_web_adapter.js'), import('./wasm-bytes.js')]);
  await adapter.default({ module_or_path: wasmBytes });
  rust = adapter;
} catch { /* The composer reports failure without downloading code. */ }
const host = await connectHost();
document.getElementById('host-badge').textContent = host.available ? `宿主已连接 · ${host.platform}` : '仅离线能力';
registerPageTools([{
  name: 'offline.state.read', description: '读取页面版本与运行状态。',
  run: () => ({ ready: !!rust, running: !document.getElementById('stop-composer').hidden, appVersion: '__FABUSHI_APP_VERSION__' }),
}]);
await mountComposer(rust, host);
