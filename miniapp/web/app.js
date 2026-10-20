const q = id => document.getElementById(id);
const MAX_FILES = 128, MAX_BYTES = 1024 ** 3;
let ready = false, running = false, generation = 0, selected = [], report = null;
let encode_packet, verify_packet, max_payload_bytes;
const bytesLabel = size => size < 1024 ? `${size} B` : size < 1024 ** 2 ? `${(size / 1024).toFixed(1)} KiB` : `${(size / 1024 ** 2).toFixed(1)} MiB`;
const textRow = (list, text) => { const row = document.createElement('li'); row.textContent = text; list.append(row); };
function controls() { q('start').disabled = !ready || running || !selected.length; q('cancel').disabled = !running; q('export').disabled = !report || running; }
function invalidate() { generation++; running = false; report = null; q('results').replaceChildren(); q('progress').value = 0; q('file-count').textContent = '0'; q('packet-count').textContent = '0'; q('byte-count').textContent = '0 B'; controls(); }
q('files').addEventListener('change', () => {
  invalidate(); selected = []; q('selection').replaceChildren();
  const files = Array.from(q('files').files);
  const total = files.reduce((sum, file) => sum + file.size, 0);
  if (files.length > MAX_FILES || total > MAX_BYTES) {
    q('files').value = ''; q('selection-summary').textContent = '选择超出限制：最多 128 个文件、合计 1 GiB'; controls(); return;
  }
  selected = files;
  for (const file of files) textRow(q('selection'), `${file.name} · ${bytesLabel(file.size)}`);
  q('selection-summary').textContent = files.length ? `${files.length} 个文件 · ${bytesLabel(total)}` : '还没有选择文件';
  q('status').textContent = ready ? '就绪。文件尚未读取。' : 'Rust 内核不可用'; controls();
});
q('cancel').onclick = () => { generation++; running = false; controls(); q('status').textContent = '已取消；未修改文件，也没有发送数据。'; };
q('start').onclick = async () => {
  if (!ready || running || !selected.length) return;
  const token = ++generation, files = [...selected], payloadLimit = max_payload_bytes();
  const transferId = crypto.getRandomValues(new Uint8Array(16));
  const summary = { schemaVersion: 1, appVersion: '__FABUSHI_APP_VERSION__', verification: 'local-packet-roundtrip', networkSent: false, status: 'running', files: [], packets: 0, verifiedBytes: 0 };
  const total = files.reduce((sum, file) => sum + file.size, 0);
  report = null; running = true; controls(); q('results').replaceChildren();
  try {
    for (let fileId = 0; fileId < files.length; fileId++) {
      const file = files[fileId]; let position = 0, index = 0;
      do {
        if (token !== generation) return;
        const payload = new Uint8Array(await file.slice(position, position + payloadLimit).arrayBuffer());
        if (token !== generation) return;
        const packet = encode_packet(payload, transferId, fileId, index, position + payload.length === file.size);
        const verified = verify_packet(packet);
        if (verified.length !== payload.length || verified.some((byte, i) => byte !== payload[i])) throw new Error('packet roundtrip mismatch');
        position += payload.length; index++; summary.packets++; summary.verifiedBytes += payload.length;
        q('packet-count').textContent = String(summary.packets); q('byte-count').textContent = bytesLabel(summary.verifiedBytes);
        q('progress').value = total ? summary.verifiedBytes / total : 0;
        if (index % 64 === 0) { q('status').textContent = `正在校验 ${file.name} · ${bytesLabel(position)} / ${bytesLabel(file.size)}`; await new Promise(resolve => setTimeout(resolve, 0)); }
      } while (position < file.size);
      if (token !== generation) return;
      summary.files.push({ name: file.name, size: file.size, packets: index, verified: true });
      textRow(q('results'), `${file.name} · ${index} 包 · 完整校验`); q('file-count').textContent = String(summary.files.length);
    }
    if (token !== generation) return;
    summary.status = 'complete'; report = summary; q('progress').value = 1;
    q('status').textContent = '本地校验完成。没有网络发送；校验和不代表身份认证。';
  } catch (error) {
    if (token === generation) q('status').textContent = `校验失败：${String(error.message || error)}`;
  } finally { if (token === generation) { running = false; controls(); } }
};
q('export').onclick = () => {
  if (!report || running) return;
  const url = URL.createObjectURL(new Blob([JSON.stringify(report, null, 2)], { type: 'application/json' }));
  const link = document.createElement('a'); link.href = url; link.download = 'global-dandelion-local-report.json'; link.click(); setTimeout(() => URL.revokeObjectURL(url), 1000);
};
const preferenceKey = 'global-dandelion-display-v1';
try { q('remember').checked = localStorage.getItem(preferenceKey) === 'true'; }
catch { q('storage-status').textContent = '当前宿主不支持持久 Web 存储；文件校验仍可使用。'; }
q('remember').onchange = () => {
  try { localStorage.setItem(preferenceKey, String(q('remember').checked)); q('storage-status').textContent = '已保存当前小程序显示偏好，不包含文件信息。'; }
  catch { q('storage-status').textContent = '保存失败；偏好仅保留到页面关闭。'; }
};
q('clear').onclick = () => {
  try { localStorage.removeItem(preferenceKey); q('remember').checked = false; q('storage-status').textContent = '已删除显示偏好。'; }
  catch { q('storage-status').textContent = '存储不可访问，无法确认删除。'; }
};
window.addEventListener('pagehide', () => { generation++; selected = []; report = null; running = false; });
try {
  const [adapter, { wasmBytes }] = await Promise.all([import('./pkg/dandelion_web_adapter.js'), import('./wasm-bytes.js')]);
  await adapter.default({ module_or_path: wasmBytes });
  ({ encode_packet, verify_packet, max_payload_bytes } = adapter);
  ready = true; q('status').textContent = '本地 Rust 内核已就绪。';
}
catch { q('status').textContent = '无法加载本地 Wasm：请检查完整安装及宿主 Wasm 策略。不会下载或运行原生程序。'; }
controls();

// --- Host capability layer -------------------------------------------------
// The page never holds paths or credentials. It receives opaque handles from
// the host after a user gesture and asks for declared tools only.
import { connectHost, registerPageTools, formatBytes } from './host-tools.js';

const host = await connectHost();
const featureSections = [...document.querySelectorAll('[data-feature]')];
for (const section of featureSections) section.hidden = !host.capabilities.has(section.dataset.feature);
q('host-badge').textContent = host.available ? `宿主已连接 · ${host.platform}` : '仅离线能力';

let transferSelection = null, transferOperation = null, transferTimer = null, scanRoot = null;
const setStatus = (id, text) => { q(id).textContent = text; };
const stopPolling = () => { if (transferTimer !== null) { clearInterval(transferTimer); transferTimer = null; } };

if (host.capabilities.has('transfer')) {
  const poolSelect = q('target-pool');
  const announced = await host.call('offline.capabilities', {});
  for (const pool of announced.ok && Array.isArray(announced.data?.targetPools) ? announced.data.targetPools : []) {
    const option = document.createElement('option');
    option.value = String(pool.id); option.textContent = String(pool.label ?? pool.id);
    poolSelect.append(option);
  }
  q('pick-transfer').onclick = async () => {
    const picked = await host.call('selection.pick', { mode: 'directory' });
    if (!picked.ok) { setStatus('transfer-status', `无法选择目录：${picked.error.message}`); return; }
    transferSelection = String(picked.data.selectionId);
    const entries = Array.isArray(picked.data.entries) ? picked.data.entries : [];
    const total = entries.reduce((sum, entry) => sum + Number(entry.size ?? 0), 0);
    setStatus('transfer-status', `已选择目录 ${entries.map(entry => entry.name).join('、') || '—'} · ${formatBytes(total)}（句柄 ${transferSelection.slice(0, 8)}…）`);
  };
  q('send').onclick = async () => {
    if (!transferSelection) { setStatus('transfer-status', '请先选择待发送文件。'); return; }
    const repeat = Math.max(1, Math.min(10000, Number(q('transfer-repeat').value || 1)));
    const watch = q('transfer-watch').checked;
    const resume = q('transfer-resume').checked;
    const sent = await host.call('transfer.send', {
      selectionId: transferSelection,
      targetPoolId: q('target-pool').value,
      ...(watch || resume ? {} : { repeat }),
      watch,
      resume,
    });
    if (!sent.ok) { setStatus('transfer-status', `传输未启动：${sent.error.message}`); return; }
    transferOperation = String(sent.data.operationId);
    q('cancel-transfer').disabled = false; q('send').disabled = true;
    setStatus('transfer-status', '传输已启动，正在等待宿主回报进度…');
    stopPolling();
    transferTimer = setInterval(async () => {
      if (!transferOperation) { stopPolling(); return; }
      const state = await host.call('transfer.status', { operationId: transferOperation });
      if (!state.ok) { setStatus('transfer-status', `进度不可用：${state.error.message}`); stopPolling(); return; }
      const phase = String(state.data?.state ?? 'unknown');
      if (phase === 'running') return;
      stopPolling();
      setStatus('transfer-status', phase === 'complete'
        ? '传输完成。'
        : phase === 'cancelled' ? '传输已停止。' : `传输失败：${String(state.data?.error ?? '未知原因')}`);
      q('cancel-transfer').disabled = true; q('send').disabled = false;
      transferOperation = null;
    }, 1500);
  };
  q('receive').onclick = async () => {
    const received = await host.call('transfer.receive', { targetPoolId: q('target-pool').value, once: q('receive-once').checked });
    setStatus('transfer-status', received.ok ? `接收已启动（${String(received.data?.operationId ?? '—').slice(0, 8)}…）。` : `接收未启动：${received.error.message}`);
  };
  q('cancel-transfer').onclick = async () => {
    if (!transferOperation) return;
    const stopped = await host.call('transfer.cancel', { operationId: transferOperation });
    stopPolling();
    setStatus('transfer-status', stopped.ok ? '已请求停止传输。' : `停止失败：${stopped.error.message}`);
    transferOperation = null; q('cancel-transfer').disabled = true; q('send').disabled = false;
  };
}

for (const button of document.querySelectorAll('[data-native-tool]')) {
  const capability = button.dataset.nativeCapability;
  if (capability && !host.capabilities.has(capability)) button.hidden = true;
  button.addEventListener('click', async () => {
    const tool = button.dataset.nativeTool;
    if (!tool) return;
    setStatus('advanced-status', `${tool} 执行中…`);
    const result = await host.call(tool, {});
    setStatus('advanced-status', result.ok
      ? `${tool} 完成：${JSON.stringify(result.data ?? {}).slice(0, 600)}`
      : `${tool} 失败：${result.error.message}`);
  });
}

if (host.capabilities.has('scan')) {
  q('pick-root').onclick = async () => {
    const picked = await host.call('selection.pick', { mode: 'directory' });
    if (!picked.ok) { setStatus('scan-status', `无法选择根目录：${picked.error.message}`); return; }
    scanRoot = String(picked.data.selectionId);
    setStatus('scan-status', `已选择根目录（句柄 ${scanRoot.slice(0, 8)}…）。`);
  };
  const runScan = (tool) => async () => {
    if (!scanRoot) { setStatus('scan-status', '请先选择根目录。'); return; }
    setStatus('scan-status', '扫描中…');
    const result = await host.call(tool, { rootId: scanRoot });
    setStatus('scan-status', result.ok
      ? `完成：${Number(result.data?.files ?? 0)} 个文件 · ${formatBytes(Number(result.data?.bytes ?? 0))}`
      : `扫描失败：${result.error.message}`);
  };
  q('scan-metadata').onclick = runScan('scan.metadata');
  q('scan-read').onclick = runScan('scan.read');
}

if (host.capabilities.has('hotspot')) {
  q('hotspot-plan').onclick = async () => {
    const planned = await host.call('hotspot.plan', {});
    setStatus('hotspot-status', planned.ok ? `方案已生成：${String(planned.data?.profileId ?? '未命名')}` : `生成失败：${planned.error.message}`);
  };
  q('hotspot-apply').onclick = async () => {
    const applied = await host.call('hotspot.apply', {});
    setStatus('hotspot-status', applied.ok ? '热点已应用（仅本程序创建的临时配置）。' : `应用失败：${applied.error.message}`);
  };
  q('hotspot-stop').onclick = async () => {
    const stopped = await host.call('hotspot.stop', {});
    setStatus('hotspot-status', stopped.ok ? '热点已停止，配置已回滚。' : `停止失败：${stopped.error.message}`);
  };
}

// Host-driven tools: read-only over page state, never a native capability.
registerPageTools([{
  name: 'offline.state.read',
  description: '读取当前页面的本地校验状态与版本。',
  run: () => ({ ready, running, selectedFiles: selected.length, packets: report?.packets ?? 0, verifiedBytes: report?.verifiedBytes ?? 0, appVersion: '__FABUSHI_APP_VERSION__' }),
}]);
