import { createGlobe } from './globe.js';
import { formatBytes } from './host-tools.js';

const $ = id => document.getElementById(id);
const MAX_BYTES = 1024 ** 3, MAX_PACKETS = 1_000_000;
export async function mountComposer(rust, host) {
  let pools = [], files = [], nativeSelection = null, nativeEntries = [], busy = false, epoch = 0, operation = null, origin = null, report = null, closed = false;
  const globe = rust ? createGlobe($('earth'), rust, text => { $('globe-state').textContent = text; $('earth-fallback').textContent = text; $('earth-fallback').hidden = false; }) : { update() {}, destroy() {} };
  const status = text => { if (!closed) $('composer-status').textContent = text; };
  const mode = () => $('mode').value;
  const nativeLocal = () => mode() === 'local' && host.capabilities.has('transfer') && pools.some(pool => pool.id === 'loopback');
  const description = {
    local: '本地转轮：在本机用 Rust 完成 GDAN 编码与解码回环，不发送网络数据。',
    global: '全球发送：仅向你已授权的 mTLS 目标传输。轨迹为运行可视化，接收校验单独确认。',
    hotspot: '热点场能：在已连接的热点／局域网内向已授权设备传输，不连接公网目标。',
  };
  const controls = () => {
    $('launch').disabled = !rust || busy; $('launch').hidden = busy; $('stop-composer').hidden = !busy;
    for (const id of ['mode', 'cycles', 'attach', 'message', 'clear-composer', 'set-origin', 'locate', 'composer-target', 'destination-lat', 'destination-lon']) $(id).disabled = busy;
    $('composer-export').disabled = !report || busy;
  };
  const list = () => {
    const entries = mode() === 'local' && !nativeLocal() ? files : nativeEntries;
    $('composer-file-list').replaceChildren();
    for (const file of entries) { const row = document.createElement('li'); row.textContent = `${file.name} · ${formatBytes(file.size)}`; $('composer-file-list').append(row); }
    $('composer-selection').textContent = entries.length ? `${entries.length} 个文件 · ${formatBytes(entries.reduce((n, f) => n + f.size, 0))}` : '尚未添加文件';
  };
  const clear = async () => {
    if (busy) return;
    epoch++;
    const previous = nativeSelection; nativeSelection = null; nativeEntries = []; files = []; report = null;
    $('composer-files').value = ''; $('message').value = ''; $('composer-progress').value = 0; list(); controls();
    if (previous) await host.call('selection.release', { selectionId: previous });
  };
  $('clear-composer').onclick = clear;
  $('composer-files').onchange = () => {
    if (busy) return;
    const picked = Array.from($('composer-files').files);
    if (picked.length > 128 || picked.reduce((n, f) => n + f.size, 0) > MAX_BYTES) { files = []; $('composer-files').value = ''; status('选择超出限制：最多 128 个文件、合计 1 GiB。'); }
    else { files = picked; report = null; status('已添加文件，点击 ↑ 才会读取和运行。'); }
    list(); controls();
  };
  $('attach').onclick = async () => {
    if (busy) return;
    if (mode() === 'local' && !nativeLocal()) { $('composer-files').click(); return; }
    if (!host.capabilities.has('selection')) { status('当前安装包没有原生文件选择能力；请安装对应平台的完整版本。'); return; }
    const ticket = epoch;
    $('attach').disabled = true;
    const result = await host.call('selection.pick', { mode: 'files' });
    if (closed || ticket !== epoch || busy) { if (result.ok) await host.call('selection.release', { selectionId: result.data?.selectionId }); return; }
    $('attach').disabled = false;
    if (!result.ok) { status(`未添加文件：${result.error.message}`); return; }
    const previous = nativeSelection;
    if (typeof result.data?.selectionId !== 'string' || !result.data.selectionId) { status('文件选择返回了无效句柄。'); return; }
    nativeSelection = result.data.selectionId; nativeEntries = Array.isArray(result.data.entries) ? result.data.entries.slice(0, 128) : [];
    if (previous) await host.call('selection.release', { selectionId: previous });
    list(); status('文件已由安装包选择，尚未发送。');
  };
  const refreshPools = async () => {
    const response = await host.call('offline.capabilities', {});
    pools = response.ok && Array.isArray(response.data?.targetPools) ? response.data.targetPools.slice(0, 16) : [];
    $('composer-target').replaceChildren();
    const eligible = pools.filter(pool => mode() === 'hotspot' ? pool.id === 'lan' || pool.kind === 'lan' : pool.id !== 'loopback' && pool.id !== 'lan' && pool.kind !== 'lan');
    for (const pool of eligible) { const option = document.createElement('option'); option.value = String(pool.id); option.textContent = String(pool.label ?? pool.id); $('composer-target').append(option); }
    if (!eligible.length) { const option = document.createElement('option'); option.value = ''; option.textContent = '未配置对应授权目标'; $('composer-target').append(option); }
  };
  $('mode').onchange = async () => {
    epoch++; await clear(); $('mode-description').textContent = description[mode()];
    globe.update({ mode: mode(), active: false, confirmed: false });
    $('globe-state').textContent = mode() === 'local' ? '本地转轮 · 就绪' : mode() === 'global' ? '全球发送 · 等待授权目标' : '热点场能 · 等待局域网目标';
    await refreshPools();
    status(mode() === 'local' ? '文字和文件均留在本机；单次运行累计校验上限 1 GiB。' : '请确认目标配置后选择文件；缺少原生适配时不会启动传输。');
  };
  const coordinates = (latitude, longitude) => {
    if (!$(latitude).value.trim() || !$(longitude).value.trim()) return null;
    const lat = Number($(latitude).value), lon = Number($(longitude).value);
    return Number.isFinite(lat) && Number.isFinite(lon) && Math.abs(lat) <= 90 && Math.abs(lon) <= 180 ? [lat, lon] : null;
  };
  const setOrigin = value => { origin = value; $('location-summary').textContent = `显示位置 ${origin[0].toFixed(3)}°, ${origin[1].toFixed(3)}° · 仅用于可视化`; globe.update({ origin }); $('route-status').textContent = '所在点已设置，不包含任何目标授权。'; };
  $('set-origin').onclick = () => { const value = coordinates('origin-lat', 'origin-lon'); if (value) setOrigin(value); else $('route-status').textContent = '请输入有效的纬度（±90）与经度（±180）。'; };
  $('locate').onclick = () => {
    if (!navigator.geolocation) { $('route-status').textContent = '此平台未提供定位，请手动输入坐标。'; return; }
    const ticket = epoch; $('route-status').textContent = '正在请求设备位置权限…';
    navigator.geolocation.getCurrentPosition(position => {
      if (closed || busy || ticket !== epoch) return;
      $('origin-lat').value = position.coords.latitude; $('origin-lon').value = position.coords.longitude; setOrigin([position.coords.latitude, position.coords.longitude]);
    }, () => { if (!closed && ticket === epoch) $('route-status').textContent = '定位未获准或不可用，请手动输入坐标。'; }, { enableHighAccuracy: false, timeout: 10000, maximumAge: 60000 });
  };
  const download = value => {
    const url = URL.createObjectURL(new Blob([JSON.stringify(value, null, 2)], { type: 'application/json' }));
    const a = document.createElement('a'); a.href = url; a.download = 'global-dandelion-session.json'; a.click(); setTimeout(() => URL.revokeObjectURL(url), 1000);
  };
  $('composer-export').onclick = () => { if (report && !busy) download(report); };
  const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
  async function localRun(ticket, repeats, text) {
    const source = [...files]; if (text) source.push(new File([text], '文字.txt', { type: 'text/plain' }));
    if (!source.length) throw new Error('请输入文字或添加文件。');
    const total = source.reduce((n, f) => n + f.size, 0);
    if (source.length > 128 || total > MAX_BYTES) throw new Error('文字和文件合计最多 128 项、1 GiB。');
    const summary = { schemaVersion: 1, mode: 'local', implementation: 'rust-gdan-v2', networkSent: false, status: 'running', requestedPasses: repeats, completedPasses: 0, verifiedBytes: 0, packets: 0, files: source.map(f => ({ name: f.name, bytes: f.size })) };
    const limit = rust.max_payload_bytes();
    for (let pass = 0; pass < repeats; pass++) {
      if (ticket !== epoch) return;
      const identity = crypto.getRandomValues(new Uint8Array(16));
      for (let id = 0; id < source.length; id++) {
        let offset = 0, index = 0; const file = source[id];
        do {
          if (ticket !== epoch) return;
          const length = Math.min(limit, file.size - offset);
          if (summary.verifiedBytes + length > MAX_BYTES || summary.packets >= MAX_PACKETS) { summary.status = 'budget_exhausted'; report = summary; status('已达到本次运行预算，停止继续循环；已完成的校验可导出。'); return; }
          const payload = new Uint8Array(await file.slice(offset, offset + limit).arrayBuffer());
          if (ticket !== epoch) return;
          const verified = rust.verify_packet(rust.encode_packet(payload, identity, id, index, offset + payload.length === file.size));
          if (verified.length !== payload.length || verified.some((byte, i) => byte !== payload[i])) throw new Error('Rust 包回环校验失败。');
          offset += payload.length; index++; summary.packets++; summary.verifiedBytes += payload.length;
          $('composer-bytes').textContent = formatBytes(summary.verifiedBytes); $('composer-packets').textContent = String(summary.packets);
          $('composer-progress').value = (pass + (total ? (source.slice(0, id).reduce((n, f) => n + f.size, 0) + offset) / total : (id + 1) / source.length)) / repeats;
          if (summary.packets % 64 === 0) { status(`本地转轮 ${pass + 1} / ${repeats} · ${file.name}`); await pause(0); }
        } while (offset < file.size);
      }
      if (ticket !== epoch) return;
      summary.completedPasses++; $('composer-pass').textContent = `${summary.completedPasses} / ${repeats}`;
      if (pass + 1 < repeats) await pause(250);
    }
    summary.status = 'complete'; report = summary; $('composer-progress').value = 1; status(`本地回环完成 · ${repeats} 次 · Rust 已校验 ${formatBytes(summary.verifiedBytes)}，没有网络发送。`);
  }
  async function nativeRun(ticket, repeats, text) {
    if (!host.capabilities.has('transfer')) throw new Error('此平台尚未安装可执行原生传输的适配；不会用动画冒充发送。');
    const targetPoolId = nativeLocal() ? 'loopback' : $('composer-target').value;
    if (!targetPoolId) throw new Error('请先在高级工具中导入已授权的目标与 mTLS 配置，然后重新选择模式。');
    let sendSelection = nativeSelection, composedSelection = null;
    if (text) {
      const composed = await host.call('selection.compose', { text, ...(nativeSelection ? { selectionId: nativeSelection } : {}) });
      if (!composed.ok) throw new Error(`文字暂存失败：${composed.error.message}`);
      if (typeof composed.data?.selectionId !== 'string' || !composed.data.selectionId) throw new Error('文字暂存返回了无效句柄。');
      sendSelection = composed.data.selectionId; composedSelection = sendSelection;
    }
    if (!sendSelection) throw new Error('请输入文字或点击 ＋ 选择本次发送的文件。');
    if (closed || ticket !== epoch) { if (composedSelection) await host.call('selection.release', { selectionId: composedSelection }); return; }
    const response = await host.call('transfer.send', { selectionId: sendSelection, targetPoolId, repeat: repeats });
    if (composedSelection) await host.call('selection.release', { selectionId: composedSelection });
    if (!response.ok) throw new Error(response.error.message);
    if (typeof response.data?.operationId !== 'string' || !response.data.operationId) throw new Error('原生启动结果缺少操作标识，无法确认执行。');
    const id = response.data.operationId;
    if (closed || ticket !== epoch) { await host.call('transfer.cancel', { operationId: id }); return; }
    operation = id;
    const destination = coordinates('destination-lat', 'destination-lon');
    globe.update({ active: false, mode: mode(), origin, destination, confirmed: false });
    $('globe-state').textContent = nativeLocal() ? '本机 UDP 回环 · Rust 校验中' : mode() === 'global' ? '发送进行中 · 等待实际接收校验' : '局域网发送中 · 场能波澜';
    status(origin && (mode() !== 'global' || destination) ? '传输已启动，轨迹表示运行过程，不代表已接收。' : '传输已启动；缺少端点坐标，暂不绘制地理轨迹。');
    let lastSequence = -1, lastProgressAt = -Infinity;
    while (!closed && ticket === epoch) {
      const result = await host.call('transfer.status', { operationId: id });
      if (closed || ticket !== epoch) return;
      if (!result.ok) { await host.call('transfer.cancel', { operationId: id }); throw new Error(`无法核对进度，已请求停止：${result.error.message}`); }
      const state = result.data;
      if (!state || !['running', 'complete', 'failed', 'cancelled'].includes(state.state)) { await host.call('transfer.cancel', { operationId: id }); throw new Error('引擎返回未知状态，已请求停止。'); }
      if (state.state === 'running') {
        const progress = state.progress;
        const verified = Number.isSafeInteger(progress?.pass_chunks_sent) && progress.pass_chunks_sent > 0;
        if (verified && Number.isSafeInteger(progress.sequence) && progress.sequence > lastSequence) { lastSequence = progress.sequence; lastProgressAt = performance.now(); }
        globe.update({ active: performance.now() - lastProgressAt < 2000 });
        if (verified) {
          $('composer-bytes').textContent = formatBytes(progress.pass_verified_payload_bytes);
          $('composer-packets').textContent = String(progress.pass_chunks_sent);
          status(`当前轮 Rust 已确认 ${progress.pass_chunks_sent} 包 · ${formatBytes(progress.pass_verified_payload_bytes)}；等待最终结果。`);
        }
        await pause(500); continue;
      }
      operation = null;
      if (state.state !== 'complete') throw new Error(state.state === 'cancelled' ? '传输已停止。' : `传输失败：${String(state.error ?? '未知原因')}`);
      const receipt = state.result ?? {};
      report = { schemaVersion: 1, mode: mode(), status: 'complete', source: 'installed-rust-engine', receipt };
      $('composer-bytes').textContent = formatBytes(Number(receipt.verified_payload_bytes ?? 0)); $('composer-packets').textContent = String(receipt.chunks_sent ?? 0); $('composer-progress').value = 1;
      $('composer-pass').textContent = `${repeats} / ${repeats}`;
      globe.update({ active: false, confirmed: Number(receipt.chunks_sent ?? 0) > 0 });
      status(`引擎已完成，实际校验 ${formatBytes(Number(receipt.verified_payload_bytes ?? 0))}。`); return;
    }
  }
  $('launch').onclick = async () => {
    if (busy || !rust) return;
    busy = true; report = null; const ticket = ++epoch, repeats = Number($('cycles').value), currentMode = mode();
    controls(); status(currentMode === 'local' ? '正在启动 Rust 本地回环…' : '正在请求安装包启动传输…'); $('composer-progress').value = 0; $('composer-packets').textContent = '0'; $('composer-bytes').textContent = '0 B'; $('composer-pass').textContent = `0 / ${repeats}`;
    try {
      if (currentMode === 'local' && !nativeLocal()) { globe.update({ active: true, mode: 'local', origin, confirmed: false }); $('globe-state').textContent = origin ? '本地回环 · 所在点波澜' : '本地回环 · 所在点未设置'; await localRun(ticket, repeats, $('message').value); }
      else await nativeRun(ticket, repeats, $('message').value);
    } catch (error) { if (ticket === epoch) status(String(error.message ?? error)); }
    finally { if (ticket === epoch) { busy = false; operation = null; globe.update({ active: false, ...(currentMode === 'local' && report?.status === 'complete' ? { confirmed: true } : {}) }); controls(); $('globe-state').textContent = report?.status === 'complete' ? '本次运行已完成' : report?.status === 'budget_exhausted' ? '已达到运行预算' : '已停止 · 等待下一次运行'; } }
  };
  $('stop-composer').onclick = async () => {
    epoch++; const id = operation; report = null; globe.update({ active: false, confirmed: false });
    if (id) {
      status('正在请求停止…'); const result = await host.call('transfer.cancel', { operationId: id });
      if (!result.ok) { $('globe-state').textContent = '停止未确认'; status(`停止请求未确认，可再次点击停止：${result.error.message}`); return; }
    }
    operation = null; busy = false; controls(); $('globe-state').textContent = '已停止'; status(id ? '已请求停止原生传输。' : '已停止本次运行。');
  };
  window.addEventListener('pagehide', () => { closed = true; epoch++; if (operation) void host.call('transfer.cancel', { operationId: operation }); if (nativeSelection) void host.call('selection.release', { selectionId: nativeSelection }); files = []; report = null; globe.destroy(); }, { once: true });
  await refreshPools();
  if (nativeLocal()) $('mode-description').textContent = '本地转轮：安装包内 Rust 引擎执行本机 UDP 回环，不连接外部目标。';
  $('globe-state').textContent = rust ? '本地转轮 · Rust 已就绪' : 'Rust 内核加载失败';
  status(rust ? '输入文字或点击 ＋ 添加文件；点击 ↑ 开始本地回环。' : '无法加载随包 Rust 内核，请检查完整安装。'); controls();
}
