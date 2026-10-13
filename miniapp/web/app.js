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
  const summary = { schemaVersion: 1, appVersion: '0.1.0', verification: 'local-packet-roundtrip', networkSent: false, status: 'running', files: [], packets: 0, verifiedBytes: 0 };
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
