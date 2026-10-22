/** Account/app-private native configuration. Never serialized to the WebView. */
import { execFileSync, spawn } from "node:child_process";
import { constants, closeSync, fstatSync, lstatSync, mkdirSync, openSync, readSync, renameSync, rmSync, statfsSync, unlinkSync, writeFileSync } from "node:fs";
import { randomUUID } from "node:crypto";
import { basename, dirname, isAbsolute, join } from "node:path";
import { isIP } from "node:net";
import { networkInterfaces } from "node:os";
import type { DandelionCliConfig } from "./miniapp-cli-backend.js";

export function reserve(directory: string, planned: number) {
  const disk = statfsSync(directory, { bigint: true });
  const total = disk.blocks * disk.bsize;
  const reserveBytes = (total + 19n) / 20n > 1073741824n ? (total + 19n) / 20n : 1073741824n;
  if (total <= 0n || disk.bavail * disk.bsize < reserveBytes + BigInt(planned)) throw new Error("DISK_RESERVE_REQUIRED");
}
function boundedFile(path: string): Buffer {
  const fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW);
  try {
    const before = fstatSync(fd, { bigint: true });
    if (!before.isFile() || before.size > 65536n) throw new Error("CONFIG_FILE_LIMIT");
    const buffer = Buffer.alloc(65537);
    const length = readSync(fd, buffer, 0, buffer.length, 0);
    const after = fstatSync(fd, { bigint: true });
    if (length > 65536 || before.size !== after.size || before.mtimeNs !== after.mtimeNs || before.ctimeNs !== after.ctimeNs) throw new Error("CONFIG_FILE_CHANGED");
    return buffer.subarray(0, length);
  } finally { closeSync(fd); }
}
type Settings = { transport: { mode: string; tls: { ca_file: string; certificate_file: string; private_key_file: string; peers: { address: string; server_name: string; class: string }[] } }; scanner: { roots: unknown[]; allowed_mount_prefixes: unknown[] }; state: { directory: string } };
export function createProfile(binaryPath: string, defaultPath: string, directory: string) {
  if (!isAbsolute(directory)) throw new Error("PRIVATE_DATA_DIRECTORY_REQUIRED");
  const info = lstatSync(directory);
  if (!info.isDirectory() || info.isSymbolicLink() || (info.mode & 0o077) !== 0) throw new Error("PRIVATE_DATA_DIRECTORY_REQUIRED");
  reserve(directory, 1024 * 1024);
  const stateDirectory = join(directory, "state");
  mkdirSync(stateDirectory, { recursive: true, mode: 0o700 });
  if (lstatSync(stateDirectory).isSymbolicLink()) throw new Error("PRIVATE_STATE_REQUIRED");
  const saved = join(directory, "service.toml");
  const receiverFile = join(directory, "receiver.json");
  const validBind = (value: unknown, requireLocal = false): value is string => {
    if (typeof value !== "string" || value.length > 80) return false;
    const parsed = /^(?:\[([^\]]+)\]|([^:]+)):(\d{1,5})$/.exec(value);
    const ip = parsed?.[1] ?? parsed?.[2];
    return !!parsed && !!ip && !!isIP(ip) && Number(parsed[3]) >= 1024 && Number(parsed[3]) <= 65535
      && (!requireLocal || Object.values(networkInterfaces()).some(entries => entries?.some(entry => entry.address === ip)));
  };
  let receiveBind = "127.0.0.1:20000";
  let receiverError: string | null = null;
  try {
    const value: unknown = JSON.parse(boundedFile(receiverFile).toString("utf8"));
    if (!validBind(value)) throw new Error("INVALID_RECEIVER_BIND");
    receiveBind = value;
  } catch (failure) { if ((failure as NodeJS.ErrnoException).code !== "ENOENT") receiverError = "接收地址配置损坏，已停用原地址；请重新授权本机接收地址。"; }
  const command = (config: string, args: string[]) => execFileSync(binaryPath, ["--config", config, "config", ...args], { encoding: "utf8", timeout: 20000, maxBuffer: 1024 * 1024, stdio: ["ignore", "pipe", "pipe"] });
  const inspect = (config: string): Settings => {
    const parsed = JSON.parse(command(config, ["check", "--json"])) as Settings;
    if (parsed.transport.mode === "tls" && [parsed.transport.tls.ca_file, parsed.transport.tls.certificate_file, parsed.transport.tls.private_key_file].some(p => !isAbsolute(p))) throw new Error("ABSOLUTE_CREDENTIAL_PATHS_REQUIRED");
    return parsed;
  };
  let configPath = defaultPath, settings = inspect(defaultPath), error: string | null = receiverError;
  try {
    boundedFile(saved);
    const candidate = inspect(saved);
    if (candidate.state.directory !== stateDirectory || candidate.scanner.roots.length !== 0 || candidate.scanner.allowed_mount_prefixes.length !== 0) throw new Error("PRIVATE_PROFILE_SCOPE_MISMATCH");
    configPath = saved; settings = candidate;
  } catch (failure) {
    if ((failure as NodeJS.ErrnoException).code !== "ENOENT") error = "配置校验失败，请重新导入；当前仅允许本机回环。";
  }
  let managed = false, managedExpiry = 0, ownedGeneration: string | null = null;
  let networkState: { state: string; message: string; receivers: number } = { state: "checking", message: "正在检查小程序内置接收节点…", receivers: 0 };
  let bootstrapChild: ReturnType<typeof spawn> | null = null;
  let bootstrapJob: Promise<void> | null = null;
  const targetPools = () => {
    const remote = settings.transport.mode === "tls" && (!managed || Date.now() < managedExpiry);
    return [{ id: "loopback", label: "本机回环验证" },
      ...(remote ? [{ id: "configured", label: managed ? `内置接收节点（${networkState.receivers} 个已验证）` : "已明确授权的 mTLS 目标" }] : []),
      ...(remote && !managed && settings.transport.tls.peers.some(peer => peer.class === "lan") ? [{ id: "lan", label: "已授权热点／局域网设备" }] : [])];
  };
  const prepareAutomatic = () => bootstrapJob ??= (async () => {
    if (configPath !== defaultPath) { networkState = { state: "manual", message: "使用已导入的授权目标", receivers: settings.transport.tls.peers.length }; return; }
    let stdout = "", length = 0;
    try {
      await new Promise<void>((resolve, reject) => {
        const child = spawn(binaryPath, ["network-prepare", "--directory", directory, "--json"], { shell: false, stdio: ["ignore", "pipe", "ignore"], env: { PATH: "/usr/local/bin:/usr/bin:/bin", LC_ALL: "C", DANDELION_LOG: "warn" } });
        bootstrapChild = child;
        const timer = setTimeout(() => { child.kill("SIGKILL"); }, 60000);
        child.stdout.on("data", (chunk: Buffer) => { length += chunk.length; if (length > 65536) child.kill("SIGKILL"); else stdout += chunk.toString("utf8"); });
        child.once("error", reject);
        child.once("close", code => { clearTimeout(timer); bootstrapChild = null; if (code === 0 && length <= 65536) resolve(); else reject(new Error("NETWORK_UNAVAILABLE")); });
      });
      const result = JSON.parse(stdout) as { config_path: string; expires_unix_seconds: number; receivers: number; authenticated: boolean };
      const path = result.config_path;
      if (typeof path !== "string" || basename(path) !== "service.toml" || dirname(dirname(path)) !== directory || !basename(dirname(path)).startsWith("network-generation-")) throw new Error("INVALID_NETWORK_PROFILE");
      ownedGeneration = dirname(path);
      if (result.authenticated !== true || !Number.isSafeInteger(result.expires_unix_seconds) || result.expires_unix_seconds * 1000 <= Date.now() || result.expires_unix_seconds * 1000 > Date.now() + 900000 || !Number.isInteger(result.receivers) || result.receivers < 1 || result.receivers > 8) throw new Error("INVALID_NETWORK_PROOF");
      const next = inspect(path);
      if (next.transport.mode !== "tls" || next.state.directory !== stateDirectory || next.scanner.roots.length || next.scanner.allowed_mount_prefixes.length) throw new Error("INVALID_NETWORK_SCOPE");
      // Keep the same backend and selection handles. Background completion must
      // not cancel or recreate a user session already running local loopback.
      if (configPath !== defaultPath) throw new Error("PROFILE_CHANGED");
      settings = next; configPath = path; managed = true; managedExpiry = result.expires_unix_seconds * 1000;
      networkState = { state: "ready", message: "内置接收节点已通过证书与协议校验", receivers: result.receivers };
    } catch {
      networkState = { state: "unavailable", message: "小程序尚无可用的内置接收节点，本地回环可继续使用。", receivers: 0 };
    }
  })();
  return {
    error: () => error,
    prepareAutomatic,
    networkStatus: () => managed && Date.now() >= managedExpiry ? { state: "expired", message: "接收节点校验已过期，请重新打开小程序连接。", receivers: 0 } : networkState,
    async close() {
      bootstrapChild?.kill("SIGKILL");
      await bootstrapJob;
      if (ownedGeneration) { rmSync(ownedGeneration, { recursive: true, force: true }); ownedGeneration = null; }
    },
    options(): DandelionCliConfig {
      return { binaryPath, get configPath() { return configPath; }, allowedRoots: [], authorizePickedRoots: true, stateDirectory,
        get targetPools() { return targetPools(); },
        get receive() {
          const tls = settings.transport.tls;
          if (managed || settings.transport.mode !== "tls") return undefined;
          return { targetPoolId: "configured", caFile: tls.ca_file, certificateFile: tls.certificate_file, privateKeyFile: tls.private_key_file, bind: receiveBind, allowedClients: [...new Set(tls.peers.map(peer => peer.address.startsWith("[") ? peer.address.slice(1, peer.address.indexOf("]")) : peer.address.slice(0, peer.address.lastIndexOf(":"))))], timeoutMs: 60000 };
        },
      };
    },
    validBind,
    configureReceiver(bind: string) {
      if (!validBind(bind, true) || settings.transport.mode !== "tls") throw new Error("RECEIVER_TLS_REQUIRED");
      reserve(directory, 1048576);
      const temporary = join(directory, `receiver-${randomUUID()}.json`);
      try { writeFileSync(temporary, JSON.stringify(bind), { flag: "wx", mode: 0o600 }); renameSync(temporary, receiverFile); receiveBind = bind; error = null; }
      finally { try { unlinkSync(temporary); } catch { /* already committed */ } }
    },
    prepareImport(path: string) {
      const bytes = boundedFile(path);
      reserve(directory, bytes.length * 4 + 1048576);
      const temporary = join(directory, `candidate-${randomUUID()}.toml`);
      const discard = () => { try { unlinkSync(temporary); } catch { /* owned temporary already gone */ } };
      try {
        writeFileSync(temporary, bytes, { flag: "wx", mode: 0o600 });
        const normalized = command(temporary, ["isolate", "--state-directory", stateDirectory]);
        if (Buffer.byteLength(normalized) > 65536) throw new Error("CONFIG_FILE_LIMIT");
        writeFileSync(temporary, normalized, { mode: 0o600 });
        const next = inspect(temporary);
        const summary = next.transport.mode === "tls" ? next.transport.tls.peers.map(peer => `${peer.address} (${peer.server_name})`).join(", ") : "本机回环，不连接外部目标";
        return { summary, discard, commit() {
          reserve(directory, 1048576);
          renameSync(temporary, saved);
          configPath = saved; settings = next; managed = false; error = null;
        } };
      } catch (failure) { discard(); throw failure; }
    },
  };
}

/** Display-safe output: native file/credential paths remain only in this process. */
export function publicResult(value: unknown, key = ""): unknown {
  if (["roots", "allowed_mount_prefixes"].includes(key) && Array.isArray(value)) return { count: value.length };
  if ((key.endsWith("_file") || ["directory", "workspace", "source"].includes(key)) && typeof value === "string") return value ? "已在本机配置" : "未配置";
  if (Array.isArray(value)) return value.map(item => publicResult(item));
  if (value && typeof value === "object") return Object.fromEntries(Object.entries(value).map(([name, item]) => [name, publicResult(item, name)]));
  return value;
}
