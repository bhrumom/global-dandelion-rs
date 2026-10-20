/** Account/app-private native configuration. Never serialized to the WebView. */
import { execFileSync } from "node:child_process";
import { constants, closeSync, fstatSync, lstatSync, mkdirSync, openSync, readSync, renameSync, statfsSync, unlinkSync, writeFileSync } from "node:fs";
import { randomUUID } from "node:crypto";
import { isAbsolute, join } from "node:path";
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
type Settings = { transport: { mode: string; tls: { ca_file: string; certificate_file: string; private_key_file: string; peers: { address: string; server_name: string }[] } }; scanner: { roots: unknown[]; allowed_mount_prefixes: unknown[] }; state: { directory: string } };
export function createProfile(binaryPath: string, defaultPath: string, directory: string) {
  if (!isAbsolute(directory)) throw new Error("PRIVATE_DATA_DIRECTORY_REQUIRED");
  const info = lstatSync(directory);
  if (!info.isDirectory() || info.isSymbolicLink() || (info.mode & 0o077) !== 0) throw new Error("PRIVATE_DATA_DIRECTORY_REQUIRED");
  reserve(directory, 1024 * 1024);
  const stateDirectory = join(directory, "state");
  mkdirSync(stateDirectory, { recursive: true, mode: 0o700 });
  if (lstatSync(stateDirectory).isSymbolicLink()) throw new Error("PRIVATE_STATE_REQUIRED");
  const saved = join(directory, "service.toml");
  const command = (config: string, args: string[]) => execFileSync(binaryPath, ["--config", config, "config", ...args], { encoding: "utf8", timeout: 20000, maxBuffer: 1024 * 1024, stdio: ["ignore", "pipe", "pipe"] });
  const inspect = (config: string): Settings => {
    const parsed = JSON.parse(command(config, ["check", "--json"])) as Settings;
    if (parsed.transport.mode === "tls" && [parsed.transport.tls.ca_file, parsed.transport.tls.certificate_file, parsed.transport.tls.private_key_file].some(p => !isAbsolute(p))) throw new Error("ABSOLUTE_CREDENTIAL_PATHS_REQUIRED");
    return parsed;
  };
  let configPath = defaultPath, settings = inspect(defaultPath), error: string | null = null;
  try {
    boundedFile(saved);
    const candidate = inspect(saved);
    if (candidate.state.directory !== stateDirectory || candidate.scanner.roots.length !== 0 || candidate.scanner.allowed_mount_prefixes.length !== 0) throw new Error("PRIVATE_PROFILE_SCOPE_MISMATCH");
    configPath = saved; settings = candidate;
  } catch (failure) {
    if ((failure as NodeJS.ErrnoException).code !== "ENOENT") error = "配置校验失败，请重新导入；当前仅允许本机回环。";
  }
  return {
    error: () => error,
    options(): DandelionCliConfig {
      const tls = settings.transport.tls;
      const remote = settings.transport.mode === "tls";
      return { binaryPath, configPath, allowedRoots: [], authorizePickedRoots: true, stateDirectory,
        targetPools: [{ id: remote ? "configured" : "loopback", label: remote ? "已明确授权的 mTLS 目标" : "本机回环验证" }],
        ...(remote ? { receive: { targetPoolId: "configured", caFile: tls.ca_file, certificateFile: tls.certificate_file, privateKeyFile: tls.private_key_file, bind: "127.0.0.1:20000", timeoutMs: 60000 } } : {}),
      };
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
          configPath = saved; settings = next; error = null;
        } };
      } catch (failure) { discard(); throw failure; }
    },
  };
}

/** Display-safe output: native file/credential paths remain only in this process. */
export function publicResult(value: unknown, key = ""): unknown {
  if (["roots", "allowed_mount_prefixes"].includes(key) && Array.isArray(value)) return { count: value.length };
  if ((key.endsWith("_file") || key === "directory") && typeof value === "string") return value ? "已在本机配置" : "未配置";
  if (Array.isArray(value)) return value.map(item => publicResult(item));
  if (value && typeof value === "object") return Object.fromEntries(Object.entries(value).map(([name, item]) => [name, publicResult(item, name)]));
  return value;
}
