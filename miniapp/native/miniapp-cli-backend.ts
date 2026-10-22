/**
 * Native owner for the Mini App tool contract.
 *
 * Execution is delegated to the already-installed `dandelion` binary with
 * argv arrays only (no shell). Every path handed to it must sit under a root
 * the Host authorized; the page can never name a path itself.
 *
 * The engine consumes roots. Explicit file selections are bounded private
 * snapshots, preserving exactly the selected set instead of its parent folder.
 */
import { spawn } from "node:child_process";
import { reserve } from "./profile.js";
import { extractLinks } from "../web/links.js";
import { stageFiles } from "./staging.js";
import { ProgressStream } from "./progress-stream.js";
import { randomUUID } from "node:crypto";
import { lstat, readdir, realpath, stat, mkdtemp, rm } from "node:fs/promises";
import { basename, dirname, isAbsolute, join, relative, sep } from "node:path";
import type {
  MiniAppCapability,
  MiniAppSelectionEntry,
  MiniAppTargetPool,
  MiniAppTransferSendConfig,
  MiniAppToolBackend,
} from "./miniapp-tools.js";

export interface DandelionCliConfig {
  /** Absolute path to the installed `dandelion` binary. */
  readonly binaryPath: string;
  /** Absolute path to the reviewed service configuration. */
  readonly configPath: string;
  /** Roots the Host owner explicitly authorized for scanning and transfer. */
  readonly allowedRoots: readonly string[];
  /** The native picker itself grants a bounded directory for bundled local-only defaults. */
  readonly authorizePickedRoots?: boolean;
  /** Authorized destination pools; empty disables transfer. */
  readonly targetPools: readonly MiniAppTargetPool[];
  /** Trusted native state directory; never supplied by the renderer. */
  readonly stateDirectory?: string;
  /** Trusted native compatibility data directory; never supplied by the renderer. */
  readonly compatDirectory?: string;
  /** Optional authenticated receive settings owned by the Host. */
  readonly receive?: { readonly targetPoolId: string; readonly caFile: string; readonly certificateFile: string; readonly privateKeyFile: string; readonly bind?: string; readonly allowedClients?: readonly string[]; readonly maxSessions?: number; readonly trafficLimitBytes?: number; readonly rateBytesPerSecond?: number; readonly timeoutMs?: number };
  /** Trusted journal directory used by hotspot status/apply/stop. */
  readonly hotspotJournalDirectory?: string;
  readonly hotspotInterface?: string;
  readonly hotspotSsid?: string;
  readonly hotspotBand?: string;
  readonly hotspotPasswordFile?: string;
  readonly updateTrust?: {
    readonly manifest?: string;
    readonly signature?: string;
    readonly publicKey: string;
    readonly directory?: string;
    readonly artifact?: string;
    readonly allowedHost?: string;
  };
  readonly enableHotspot?: boolean;
  readonly enableUpdate?: boolean;
  readonly timeoutMs?: number;
  /** Host platform. Apple requires the live native contract probe before invocation. */
  readonly platform?: NodeJS.Platform;
}

export interface MiniAppPickPort {
  (mode: "files" | "directory", limits: { readonly maxFiles: number; readonly maxBytes: number }): Promise<{ readonly path: string; readonly name: string; readonly size: number }[] | null>;
}

export interface MiniAppChildHandle {
  readonly done: Promise<{ readonly code: number; readonly stdout: string; readonly stderr: string }>;
  kill(): void;
  snapshot?(): string;
}

export interface MiniAppSpawnPort {
  (argv: readonly string[]): MiniAppChildHandle;
}

const APPLE_NATIVE_CONTRACT = "dandelion.native.apple.v1";

const SELECTION_TTL_MS = 10 * 60_000;
const MAX_SELECTIONS = 16;
const MAX_OPERATIONS = 8;

function insideRoot(root: string, target: string): boolean {
  const rel = relative(root, target);
  return rel === "" || (!rel.startsWith(`..${sep}`) && rel !== ".." && !isAbsolute(rel));
}

function defaultSpawn(binaryPath: string, configPath: string): MiniAppSpawnPort {
  return (argv) => {
    const child = spawn(binaryPath, ["--config", configPath, ...argv], {
      shell: false,
      windowsHide: true,
      stdio: ["ignore", "pipe", "pipe"],
      env: { PATH: "/usr/local/bin:/usr/bin:/bin", LC_ALL: "C", DANDELION_LOG: "warn" },
    });
    let stdout = "", stderr = "", stdoutBytes = 0, stderrBytes = 0;
    const progress = argv.includes("--progress-json") ? new ProgressStream() : null;
    const output = () => progress ? progress.snapshot() : stdout;
    let exceeded = false;
    let killTimer: ReturnType<typeof setTimeout> | undefined;
    let settled = false;
    const kill = (): void => {
      if (settled || killTimer) return;
      child.kill("SIGTERM");
      killTimer = setTimeout(() => { if (!settled) child.kill("SIGKILL"); }, 3000);
    };
    child.stdout.on("data", (chunk: Buffer) => {
      if (progress) {
        try { progress.push(chunk); } catch { exceeded = true; kill(); }
        return;
      }
      stdoutBytes += chunk.length;
      if (stdoutBytes > 1_048_576) { exceeded = true; kill(); }
      else stdout += chunk.toString("utf8");
    });
    child.stderr.on("data", (chunk: Buffer) => {
      stderrBytes += chunk.length;
      if (stderrBytes > 65_536) { exceeded = true; kill(); }
      else stderr += chunk.toString("utf8");
    });
    const done = new Promise<{ code: number; stdout: string; stderr: string }>((settle) => {
      child.on("error", (error) => settle({ code: -1, stdout: output(), stderr: String(error.message) }));
      child.on("close", (code) => {
        settled = true;
        if (killTimer) clearTimeout(killTimer);
        settle({ code: exceeded ? -2 : code ?? -1, stdout: output(), stderr });
      });
    });
    return { done, kill, snapshot: output };
  };
}

interface NativeRun {
  readonly handle: MiniAppChildHandle;
  result: Promise<{ readonly code: number; readonly stdout: string; readonly stderr: string }>;
  cancelled: boolean;
  timedOut: boolean;
}

interface Operation {
  readonly run: NativeRun;
  state: "running" | "complete" | "failed" | "cancelled";
  result?: unknown;
  error?: string;
  mode: "single" | "receive" | "send";
  download?: boolean;
}

export function createDandelionCliBackend(
  config: DandelionCliConfig,
  ports: { readonly pick: MiniAppPickPort; readonly spawn?: MiniAppSpawnPort },
): MiniAppToolBackend {
  const timeoutMs = config.timeoutMs ?? 30 * 60_000;
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 30 * 60_000) throw new Error("INVALID_NATIVE_TIMEOUT");
  const platform = config.platform ?? process.platform;
  const nativeCliSupported = platform === "linux" || platform === "darwin";
  const spawnCommand = ports.spawn ?? defaultSpawn(config.binaryPath, config.configPath);
  const selections = new Map<string, { root: string; device: bigint; inode: bigint; expiresAt: number; users?: number; released?: boolean; dispose?: () => Promise<void> }>();
  const operations = new Map<string, Operation>();
  const active = new Set<NativeRun>();
  let generation = 0;
  let downloadAdmitting = false;
  const downloadJobs = new Set<Promise<void>>();
  const stagingJobs = new Set<Promise<Awaited<ReturnType<typeof stageFiles>>>>();
  const stageSnapshot = (paths: string[], limits: { maxFiles: number; maxBytes: number }, text = '') => {
    if (stagingJobs.size) throw new Error("STAGING_BUSY");
    const ticket = generation;
    const job = (async () => {
      const staged = await stageFiles(paths, stateDirectory!, limits, text);
      if (ticket !== generation) { await staged.dispose(); throw new Error("DANDELION_CANCELLED"); }
      return staged;
    })();
    stagingJobs.add(job);
    void job.then(() => stagingJobs.delete(job), () => stagingJobs.delete(job));
    return job;
  };
  const roots = config.allowedRoots.filter((root) => isAbsolute(root));
  const stateDirectory = config.stateDirectory && isAbsolute(config.stateDirectory) ? config.stateDirectory : null;
  const compatDirectory = config.compatDirectory && isAbsolute(config.compatDirectory) ? config.compatDirectory : null;
  const compatPath = async (value: unknown, fallback: string): Promise<string> => {
    if (!compatDirectory) throw new Error("COMPAT_DISABLED");
    const raw = typeof value === "string" && value.length > 0 ? value : fallback;
    const candidate = isAbsolute(raw) ? raw : `${compatDirectory}/${raw}`;
    const resolved = await realpath(candidate);
    const root = await realpath(compatDirectory);
    if (!insideRoot(root, resolved)) throw new Error("COMPAT_PATH_NOT_AUTHORIZED");
    return resolved;
  };
  // macOS resolves /tmp to /private/tmp; compare real paths on both sides.
  let resolvedRoots: Promise<string[]> | null = null;
  const canonicalRoots = (): Promise<string[]> => {
    resolvedRoots ??= Promise.all(roots.map(async (root) => {
      try { return await realpath(root); } catch { return root; }
    }));
    return resolvedRoots;
  };

  const dropSelection = (id: string) => {
    const record = selections.get(id); selections.delete(id);
    if (record) { record.released = true; if (!record.users) void record.dispose?.().catch(() => {}); }
  };
  const pruneSelections = (): void => {
    const now = Date.now();
    for (const [id, record] of selections) if (record.expiresAt <= now && !record.users) dropSelection(id);
    while (selections.size > MAX_SELECTIONS) {
      const oldest = selections.keys().next().value;
      if (oldest === undefined) break;
      dropSelection(oldest);
    }
  };

  const pruneOperations = (): void => {
    while (operations.size >= MAX_OPERATIONS) {
      const oldest = [...operations].find(([, operation]) => operation.state !== "running")?.[0];
      if (oldest === undefined) break;
      operations.delete(oldest);
    }
  };

  const requireSelection = async (selectionId: string): Promise<string> => {
    pruneSelections();
    const record = selections.get(selectionId);
    if (!record) throw new Error("UNKNOWN_SELECTION");
    if (record.expiresAt <= Date.now()) { dropSelection(selectionId); throw new Error("SELECTION_EXPIRED"); }
    const info = await lstat(record.root, { bigint: true });
    if (!info.isDirectory() || info.isSymbolicLink() || info.dev !== record.device || info.ino !== record.inode) throw new Error("SELECTION_SOURCE_CHANGED");
    return record.root;
  };

  const admit = (argv: readonly string[], continuous = false): NativeRun => {
    if (active.size >= MAX_OPERATIONS) throw new Error("NATIVE_OPERATION_LIMIT");
    const handle = spawnCommand(argv);
    const run: NativeRun = { handle, result: handle.done, cancelled: false, timedOut: false };
    active.add(run);
    const timer = continuous ? null : setTimeout(() => { run.timedOut = true; handle.kill(); }, timeoutMs);
    run.result = handle.done.finally(() => { if (timer) clearTimeout(timer); active.delete(run); });
    return run;
  };

  const parseResult = (text: string, mode: "single" | "receive" | "send" | "watch" = "single"): unknown => {
    const trimmed = text.trim();
    try {
      if (mode === "single") return trimmed ? JSON.parse(trimmed) : {};
      const records: unknown[] = trimmed.split(/\r?\n/).map((line) => JSON.parse(line));
      if (mode === "send") {
        if (!records.length || records.length > 1801 || records.slice(0, -1).some(record => !record || typeof record !== "object" || Reflect.get(record, "event") !== "progress")) throw new Error();
        const final = records.at(-1);
        if (!final || typeof final !== "object" || Reflect.get(final, "schema_version") !== 1) throw new Error();
        return final;
      }
      if (mode === "watch") {
        if (records.length < 1 || records.length > 10_000) throw new Error();
        return { samples: records };
      }
      if (records.length !== 2) throw new Error();
      const [ready, report] = records as Record<string, unknown>[];
      if (!ready || ready.event !== "ready" || ready.mutual_tls !== true || !report
        || typeof report.sessions !== "number" || typeof report.failed_sessions !== "number"
        || typeof report.verified_payload_bytes !== "number" || typeof report.cancelled !== "boolean") throw new Error();
      return report;
    } catch { throw new Error("DANDELION_INVALID_JSON"); }
  };

  const finish = async (run: NativeRun, mode: "single" | "receive" | "send" | "watch" = "single"): Promise<unknown> => {
    let result;
    try { result = await run.result; }
    catch { throw new Error("DANDELION_PROCESS_FAILED"); }
    if (run.cancelled) throw new Error("DANDELION_CANCELLED");
    if (run.timedOut) throw new Error("DANDELION_TIMEOUT");
    if (result.code !== 0) {
      const safeErrors: Record<string, string> = {
        "invalid configuration: invalid TLS credentials or trust configuration": "TLS_CREDENTIALS_INVALID",
        "invalid configuration: credentials must be bounded regular files; private keys require private permissions and no hard links": "CREDENTIAL_FILE_POLICY",
        "invalid configuration: receiver requires bounded sessions, timeout, rate and positive traffic budget": "RECEIVER_LIMITS_INVALID",
        "invalid configuration: non-loopback listen requires --allow-remote and an explicit --allowed-client list": "RECEIVER_AUTH_REQUIRED",
      };
      let category: string | undefined;
      try { category = safeErrors[JSON.parse(result.stderr.trim().split("\n").at(-1)!).error]; } catch { /* raw diagnostics never cross the bridge */ }
      throw new Error(category ?? `DANDELION_EXIT_${result.code}`);
    }
    return parseResult(result.stdout, mode);
  };

  const requireNative = async (): Promise<void> => {
    if (!nativeCliSupported) throw new Error("PLATFORM_UNSUPPORTED");
    if (platform === "darwin") {
      const doctor = await finish(admit(["doctor", "--json"])) as Record<string, unknown>;
      if (doctor?.native_contract !== APPLE_NATIVE_CONTRACT || doctor.os !== "macos"
        || typeof doctor.target !== "string" || !doctor.target.endsWith("-apple-darwin")) {
        throw new Error("NATIVE_RUNTIME_INCOMPATIBLE");
      }
    }
  };

  const execute = async (ticket: number, argv: readonly string[], mode: "single" | "watch" = "single"): Promise<unknown> => {
    if (ticket !== generation) throw new Error("DANDELION_CANCELLED");
    return finish(admit(argv), mode);
  };

  const startOperation = (argv: readonly string[], mode: "single" | "receive" | "send"): { operationId: string } => {
    pruneOperations();
    if (operations.size >= MAX_OPERATIONS) throw new Error("NATIVE_OPERATION_LIMIT");
    const operationId = randomUUID();
    const run = admit(argv, mode === "send" && argv.includes("--watch"));
    const operation: Operation = { run, state: "running", mode };
    operations.set(operationId, operation);
    void finish(run, mode).then((result) => {
      if (operation.state === "cancelled") return;
      operation.state = "complete";
      operation.result = result;
    }, (error: unknown) => {
      if (operation.state === "cancelled") return;
      operation.state = "failed";
      operation.error = error instanceof Error ? error.message : "DANDELION_PROCESS_FAILED";
    });
    return { operationId };
  };

  return {
    async capabilities() {
      let available = false;
      try { await requireNative(); available = true; } catch { /* keep offline operation available */ }
      const capabilities: MiniAppCapability[] = ["offline", "selection"];
      if (available) {
        if (config.targetPools.length > 0 && (roots.length > 0 || config.authorizePickedRoots === true)) capabilities.push("transfer");
        if (roots.length > 0 || config.authorizePickedRoots === true) capabilities.push("scan");
        if (stateDirectory) capabilities.push("state");
        capabilities.push("config");
        if (platform === "linux") capabilities.push("platform");
        capabilities.push("diagnostics");
        if (compatDirectory) capabilities.push("compat");
        if (platform === "linux" && config.enableHotspot === true) capabilities.push("hotspot");
        if (config.enableUpdate === true) capabilities.push("update");
      }
      return Promise.resolve({ capabilities, platform: "desktop", targetPools: [...config.targetPools], features: available && stateDirectory ? ["link-download-v1"] : [] });
    },
    readState() {
      return Promise.resolve({
        selections: selections.size,
        authorizedRoots: roots.length,
        targetPools: config.targetPools.length,
        operations: [...operations].map(([id, operation]) => ({ id, state: operation.state })),
      });
    },
    async pick(mode, limits) {
      const ticket = generation;
      if (mode === "files" && !stateDirectory) throw new Error("PRIVATE_STAGING_REQUIRED");
      const picked = await ports.pick(mode, limits);
      if (!picked || picked.length === 0) return null;
      if (mode === "files") {
        if (config.authorizePickedRoots !== true) {
          const allowed = await canonicalRoots();
          for (const file of picked) { const canonical = await realpath(file.path); if (!allowed.some(root => insideRoot(root, canonical))) throw new Error("ROOT_NOT_AUTHORIZED"); }
        }
        const staged = await stageSnapshot(picked.map(file => file.path), limits);
        if (ticket !== generation) { await staged.dispose(); throw new Error("DANDELION_CANCELLED"); }
        pruneSelections();
        if (selections.size >= MAX_SELECTIONS) { await staged.dispose(); throw new Error("SELECTION_LIMIT_EXCEEDED"); }
        const info = await stat(staged.root, { bigint: true }), selectionId = randomUUID();
        selections.set(selectionId, { root: staged.root, device: info.dev, inode: info.ino, expiresAt: Date.now() + SELECTION_TTL_MS, dispose: staged.dispose });
        return { selectionId, entries: staged.entries };
      }
      const root = await realpath(picked[0]!.path);
      const info = await stat(root, { bigint: true });
      if (dirname(root) === root) throw new Error("FILESYSTEM_ROOT_NOT_AUTHORIZED");
      if (mode === "directory" && !info.isDirectory()) throw new Error("DIRECTORY_SELECTION_REQUIRED");
      if (config.authorizePickedRoots !== true && !(await canonicalRoots()).some((allowed) => insideRoot(allowed, root))) throw new Error("ROOT_NOT_AUTHORIZED");
      // Directory stat size is filesystem metadata, not the recursive payload
      // size. The Dandelion scanner enforces the configured recursive budget.
      if (picked.length > limits.maxFiles || (!info.isDirectory() && info.size > limits.maxBytes)) throw new Error("SELECTION_LIMIT_EXCEEDED");
      if (ticket !== generation) throw new Error("DANDELION_CANCELLED");
      pruneSelections();
      const selectionId = randomUUID();
      selections.set(selectionId, { root, device: info.dev, inode: info.ino, expiresAt: Date.now() + SELECTION_TTL_MS });
      const entries: MiniAppSelectionEntry[] = [{ name: basename(root), size: picked[0]!.size }];
      return { selectionId, entries };
    },
    async downloadLinks(text, selectionId) {
      if (downloadAdmitting) throw new Error("DOWNLOAD_BUSY");
      downloadAdmitting = true;
      try {
      if (!stateDirectory || downloadJobs.size || stagingJobs.size || Buffer.byteLength(text) > 65536) throw new Error("DOWNLOAD_BUSY_OR_LIMIT");
      const urls = extractLinks(text);
      if (!urls.length || urls.length > 8) throw new Error("DOWNLOAD_LINK_LIMIT");
      const ticket = generation;
      await requireNative();
      if (ticket !== generation || downloadJobs.size) throw new Error("DANDELION_CANCELLED");
      const previous = selectionId ? selections.get(selectionId) : undefined;
      if (selectionId && (!previous || !previous.dispose)) throw new Error("STAGED_SELECTION_REQUIRED");
      if (previous) await requireSelection(selectionId!);
      pruneOperations();
      if (operations.size >= MAX_OPERATIONS) throw new Error("NATIVE_OPERATION_LIMIT");
      const parentInfo = await lstat(stateDirectory);
      if (!parentInfo.isDirectory() || parentInfo.isSymbolicLink() || (parentInfo.mode & 0o077) !== 0) throw new Error("PRIVATE_STAGING_REQUIRED");
      if ((await readdir(stateDirectory)).filter(name => name.startsWith("download-")).length >= 8) throw new Error("DOWNLOAD_CLEANUP_REQUIRED");
      reserve(stateDirectory, 1024 * 1024);
      const directory = await mkdtemp(join(stateDirectory, "download-"));
      if (ticket !== generation) { await rm(directory, { recursive: true, force: true }); throw new Error("DANDELION_CANCELLED"); }
      if (previous) previous.users = (previous.users ?? 0) + 1;
      const id = randomUUID();
      let run: NativeRun;
      try { run = admit(["download", "--directory", directory, ...urls.flatMap(url => ["--url", url]), "--progress-json", "--json"]); }
      catch (error) { if (previous) previous.users!--; await rm(directory, { recursive: true, force: true }); throw error; }
      const operation: Operation = { run, state: "running", mode: "send", download: true };
      operations.set(id, operation);
      const timer = setTimeout(() => { run.timedOut = true; run.handle.kill(); }, 120000);
      let job: Promise<void>;
      job = (async () => {
        let staged: Awaited<ReturnType<typeof stageFiles>> | undefined;
        try {
          const receipt = await finish(run, "send") as { entries: { name: string; file: string; size: number }[]; warnings: string[] };
          if (!Array.isArray(receipt.entries) || receipt.entries.length < 1 || receipt.entries.length > 32 || receipt.entries.some(e => !/^resource-[0-9]{3}\.(bin|html)$/.test(e.file) || typeof e.name !== "string" || e.name.length > 160 || !Number.isSafeInteger(e.size) || e.size < 0)) throw new Error("INVALID_DOWNLOAD_RECEIPT");
          if (ticket !== generation || operation.state === "cancelled") return;
          const source = previous ? (await readdir(previous.root)).map(name => join(previous.root, name)) : [];
          const paths = receipt.entries.map(entry => join(directory, entry.file));
          for (const [index, path] of paths.entries()) { const info = await lstat(path); if (!info.isFile() || info.isSymbolicLink() || info.size !== receipt.entries[index]!.size) throw new Error("INVALID_DOWNLOAD_FILE"); }
          staged = await stageSnapshot([...source, ...paths], { maxFiles: 128, maxBytes: 1024 ** 3 });
          if (ticket !== generation || operation.state === "cancelled") return;
          if (run.timedOut) throw new Error("DOWNLOAD_TIMEOUT");
          pruneSelections();
          if (selections.size >= MAX_SELECTIONS) throw new Error("SELECTION_LIMIT_EXCEEDED");
          const info = await stat(staged.root, { bigint: true }), selectionId = randomUUID();
          const entries = staged.entries.map((entry, index) => index < source.length ? entry : { name: receipt.entries[index - source.length]!.name, size: entry.size });
          selections.set(selectionId, { root: staged.root, device: info.dev, inode: info.ino, expiresAt: Date.now() + SELECTION_TTL_MS, dispose: staged.dispose });
          staged = undefined;
          operation.result = { selectionId, entries, warnings: Array.isArray(receipt.warnings) ? receipt.warnings.slice(0, 32).map(w => String(w).slice(0, 240)) : [] };
        } catch (error) {
          if (operation.state !== "cancelled") { operation.state = "failed"; operation.error = run.timedOut ? "下载超时，请检查链接或资源大小。" : "下载未完成，请检查链接访问权限、网络或文件大小。"; }
        } finally {
          clearTimeout(timer);
          try {
            await staged?.dispose();
            if (previous) { previous.users!--; if (previous.released && !previous.users) await previous.dispose?.(); }
            await rm(directory, { recursive: true, force: true });
          } catch { operation.state = "failed"; operation.error = "下载暂存清理未完成，请关闭小程序后检查可用空间。"; }
        }
        if (operation.state === "running" && operation.result) operation.state = "complete";
      })();
      downloadJobs.add(job);
      void job.then(() => downloadJobs.delete(job), () => downloadJobs.delete(job));
      return { operationId: id };
      } finally { downloadAdmitting = false; }
    },
    async compose(text, selectionId) {
      if (!stateDirectory || Buffer.byteLength(text) > 65536 || !text) throw new Error("MESSAGE_LIMIT");
      const ticket = generation;
      const previous = selectionId ? selections.get(selectionId) : undefined;
      if (selectionId && (!previous || !previous.dispose)) throw new Error("STAGED_SELECTION_REQUIRED");
      if (previous) previous.users = (previous.users ?? 0) + 1;
      let staged;
      try {
        const source = previous ? (await readdir(await requireSelection(selectionId!))).map(name => join(previous.root, name)) : [];
        staged = await stageSnapshot(source, { maxFiles: 128, maxBytes: 1024 ** 3 }, text);
      } finally { if (previous) { previous.users!--; if (previous.released && !previous.users) void previous.dispose?.().catch(() => {}); } }
      if (ticket !== generation) { await staged.dispose(); throw new Error("DANDELION_CANCELLED"); }
      pruneSelections();
      if (selections.size >= MAX_SELECTIONS) { await staged.dispose(); throw new Error("SELECTION_LIMIT_EXCEEDED"); }
      const info = await stat(staged.root, { bigint: true }), id = randomUUID();
      selections.set(id, { root: staged.root, device: info.dev, inode: info.ino, expiresAt: Date.now() + SELECTION_TTL_MS, dispose: staged.dispose });
      return { selectionId: id, entries: staged.entries };
    },
    release(selectionId) {
      const download = operations.get(selectionId);
      if (download?.download && download.state === "running") { download.state = "cancelled"; download.run.cancelled = true; download.run.handle.kill(); }
      if (download?.download && download.state === "complete" && download.result && typeof download.result === "object") {
        const completed = Reflect.get(download.result, "selectionId");
        if (typeof completed === "string") dropSelection(completed);
        download.state = "cancelled";
      }
      dropSelection(selectionId);
      return Promise.resolve();
    },
    async send(sendConfig: MiniAppTransferSendConfig) {
      const ticket = generation;
      await requireNative();
      if (ticket !== generation) throw new Error("DANDELION_CANCELLED");
      if (platform !== "linux" && sendConfig.resume === true) throw new Error("PLATFORM_UNSUPPORTED");
      const root = await requireSelection(sendConfig.selectionId);
      if (ticket !== generation) throw new Error("DANDELION_CANCELLED");
      if (!config.targetPools.some((pool) => pool.id === sendConfig.targetPoolId)) throw new Error("UNKNOWN_TARGET_POOL");
      const argv = ["run", "--source", root];
      if (sendConfig.targetPoolId === "loopback") argv.push("--local-loopback");
      if (sendConfig.targetPoolId === "lan") argv.push("--lan-only");
      if (sendConfig.repeat !== undefined) argv.push("--repeat", String(sendConfig.repeat));
      if (sendConfig.watch === true) argv.push("--watch");
      if (sendConfig.intervalMs !== undefined) argv.push("--interval-ms", String(sendConfig.intervalMs));
      if (sendConfig.resume === true) argv.push("--resume");
      if (sendConfig.rateBytesPerSecond !== undefined) argv.push("--rate-bytes-per-second", String(sendConfig.rateBytesPerSecond));
      if (sendConfig.trafficLimitBytes !== undefined) argv.push("--traffic-limit-bytes", String(sendConfig.trafficLimitBytes));
      if (sendConfig.queueCapacity !== undefined) argv.push("--queue-capacity", String(sendConfig.queueCapacity));
      if (sendConfig.chunkBytes !== undefined) argv.push("--chunk-bytes", String(sendConfig.chunkBytes));
      argv.push("--progress-json", "--json");
      const record = selections.get(sendConfig.selectionId);
      if (!record) throw new Error("UNKNOWN_SELECTION");
      record.users = (record.users ?? 0) + 1;
      try {
        const result = startOperation(argv, "send");
        const release = () => { record.users!--; if (record.released && !record.users) void record.dispose?.().catch(() => {}); };
        void operations.get(result.operationId)!.run.handle.done.then(release, release);
        return result;
      } catch (error) { record.users!--; throw error; }
    },
    async receive(receiveConfig) {
      const ticket = generation;
      await requireNative();
      if (ticket !== generation) throw new Error("DANDELION_CANCELLED");
      const settings = config.receive;
      if (!settings || settings.targetPoolId !== receiveConfig.targetPoolId) throw new Error("RECEIVE_DISABLED");
      const argv = ["receive", "--bind", settings.bind ?? "127.0.0.1:20000", "--ca-file", settings.caFile, "--certificate-file", settings.certificateFile, "--private-key-file", settings.privateKeyFile];
      if (settings.allowedClients?.length) {
        if (settings.allowedClients.length > 8) throw new Error("RECEIVER_CLIENT_LIMIT");
        argv.push("--allow-remote");
        for (const address of settings.allowedClients) argv.push("--allowed-client", address);
      }
      const maxSessions = receiveConfig.maxSessions ?? settings.maxSessions;
      const trafficLimitBytes = receiveConfig.trafficLimitBytes ?? settings.trafficLimitBytes;
      const rateBytesPerSecond = receiveConfig.rateBytesPerSecond ?? settings.rateBytesPerSecond;
      const receiveTimeoutMs = receiveConfig.timeoutMs ?? settings.timeoutMs;
      if (maxSessions !== undefined) argv.push("--max-sessions", String(maxSessions));
      if (trafficLimitBytes !== undefined) argv.push("--traffic-limit-bytes", String(trafficLimitBytes));
      if (rateBytesPerSecond !== undefined) argv.push("--rate-bytes-per-second", String(rateBytesPerSecond));
      if (receiveTimeoutMs !== undefined) argv.push("--timeout-ms", String(receiveTimeoutMs));
      if (receiveConfig.once !== false) argv.push("--once");
      argv.push("--json");
      return startOperation(argv, "receive");
    },
    async status(operationId) {
      const operation = operations.get(operationId);
      if (!operation) throw new Error("UNKNOWN_OPERATION");
      let ready: { address: string; mutual_tls: true } | undefined;
      if (operation.mode === "receive" && operation.run.handle.snapshot) {
        try {
          const first = JSON.parse(operation.run.handle.snapshot().split("\n")[0]!);
          if (first.event === "ready" && first.mutual_tls === true && typeof first.address === "string" && first.address.length <= 80) ready = { address: first.address, mutual_tls: true };
        } catch { /* readiness is not complete yet */ }
      }
      let progress: { sequence: number; pass_verified_payload_bytes: number; pass_chunks_sent: number } | undefined;
      if (operation.mode === "send" && operation.run.handle.snapshot) {
        const raw = operation.run.handle.snapshot(), end = raw.lastIndexOf("\n");
        try {
          const frame = JSON.parse(raw.slice(raw.lastIndexOf("\n", end - 1) + 1, end));
          if (frame.event === "progress" && [frame.sequence, frame.pass_verified_payload_bytes, frame.pass_chunks_sent].every(value => Number.isSafeInteger(value) && value >= 0)) progress = { sequence: frame.sequence, pass_verified_payload_bytes: frame.pass_verified_payload_bytes, pass_chunks_sent: frame.pass_chunks_sent };
        } catch { /* a partially written progress line is not evidence */ }
      }
      return { state: operation.state, result: operation.result, error: operation.error, ready, progress, ...(operation.download ? { kind: "download" } : {}) };
    },
    async cancel(operationId) {
      const operation = operations.get(operationId);
      if (!operation) throw new Error("UNKNOWN_OPERATION");
      operation.state = "cancelled";
      operation.run.cancelled = true;
      operation.run.handle.kill();
    },
    async cancelAll() {
      generation += 1;
      const owned = [...selections.values()], runs = [...active], stages = [...stagingJobs], downloads = [...downloadJobs];
      for (const operation of operations.values()) {
        if (operation.state === "running") operation.state = "cancelled";
      }
      for (const run of active) { run.cancelled = true; run.handle.kill(); }
      operations.clear();
      for (const id of selections.keys()) dropSelection(id);
      await Promise.allSettled([...runs.map(run => run.handle.done), ...stages, ...downloads]);
      await Promise.all(owned.map(record => record.dispose?.()));
    },
    async scan(mode, rootId) {
      const ticket = generation;
      await requireNative();
      if (ticket !== generation) throw new Error("DANDELION_CANCELLED");
      return execute(ticket, ["scan", mode, "--source", await requireSelection(rootId), "--json"]);
    },
    async state(action) {
      const ticket = generation;
      await requireNative();
      if (ticket !== generation) throw new Error("DANDELION_CANCELLED");
      if (!stateDirectory) throw new Error("STATE_DISABLED");
      if (action === "inspect") return execute(ticket, ["state", "inspect", "--directory", stateDirectory, "--json"]);
      const picked = await ports.pick("files", { maxFiles: 1, maxBytes: 1024 * 1024 });
      if (!picked || picked.length === 0) throw new Error("STATE_IMPORT_CANCELLED");
      const file = await realpath(picked[0]!.path);
      const fileInfo = await stat(file);
      if (!fileInfo.isFile()) throw new Error("STATE_IMPORT_FILE_REQUIRED");
      if (fileInfo.size > 1024 * 1024) throw new Error("STATE_IMPORT_TOO_LARGE");
      return execute(ticket, ["state", "import", "--directory", stateDirectory, "--file", file]);
    },
    async config(action) {
      const ticket = generation;
      await requireNative();
      if (ticket !== generation) throw new Error("DANDELION_CANCELLED");
      return execute(ticket, ["config", action === "defaults" ? "init" : "check", "--json"]);
    },
    async platform(action) {
      if (platform !== "linux") throw new Error("PLATFORM_UNSUPPORTED");
      const ticket = generation;
      await requireNative();
      if (ticket !== generation) throw new Error("DANDELION_CANCELLED");
      const argv = action === "watch"
        ? ["platform", "watch", "--samples", "5", "--interval-ms", "1000", "--json"]
        : ["platform", action, "--json"];
      return execute(ticket, argv, action === "watch" ? "watch" : "single");
    },
    async hotspot(action) {
      if (platform !== "linux") throw new Error("PLATFORM_UNSUPPORTED");
      const ticket = generation;
      await requireNative();
      if (ticket !== generation) throw new Error("DANDELION_CANCELLED");
      if (config.enableHotspot !== true) throw new Error("HOTSPOT_DISABLED");
      const iface = config.hotspotInterface;
      if (action === "plan") {
        if (!iface) throw new Error("HOTSPOT_INTERFACE_NOT_CONFIGURED");
        return execute(ticket, ["hotspot", "plan", "--interface", iface, "--ssid", config.hotspotSsid ?? "Global-Dandelion", "--band", config.hotspotBand ?? "a", "--json"]);
      }
      const journalDirectory = config.hotspotJournalDirectory;
      if (!journalDirectory || !isAbsolute(journalDirectory)) throw new Error("HOTSPOT_JOURNAL_NOT_CONFIGURED");
      if (action === "status") return execute(ticket, ["hotspot", "status", "--journal-directory", journalDirectory, "--json"]);
      if (action === "stop") return execute(ticket, ["hotspot", "stop", "--journal-directory", journalDirectory, "--confirm", "--json"]);
      if (!iface || !config.hotspotPasswordFile) throw new Error("HOTSPOT_APPLY_NOT_CONFIGURED");
      return execute(ticket, ["hotspot", "apply", "--interface", iface, "--ssid", config.hotspotSsid ?? "Global-Dandelion", "--band", config.hotspotBand ?? "a", "--journal-directory", journalDirectory, "--password-file", config.hotspotPasswordFile, "--confirm", "--json"]);
    },
    async update(action) {
      const ticket = generation;
      await requireNative();
      if (ticket !== generation) throw new Error("DANDELION_CANCELLED");
      if (config.enableUpdate !== true) throw new Error("UPDATE_DISABLED");
      const trust = config.updateTrust;
      if (!trust?.publicKey) throw new Error("UPDATE_TRUST_NOT_CONFIGURED");
      const argv = ["update", action];
      if (trust.manifest) argv.push("--manifest", trust.manifest);
      if (trust.signature) argv.push("--signature", trust.signature);
      argv.push("--public-key", trust.publicKey);
      if (action === "stage") {
        if (!trust.directory) throw new Error("UPDATE_DIRECTORY_NOT_CONFIGURED");
        argv.push("--directory", trust.directory);
        if (trust.artifact) argv.push("--artifact", trust.artifact);
        else if (trust.allowedHost) argv.push("--allowed-host", trust.allowedHost);
        else throw new Error("UPDATE_SOURCE_NOT_CONFIGURED");
      }
      argv.push("--json");
      return execute(ticket, argv);
    },
    async diagnostics(action) {
      const ticket = generation;
      await requireNative();
      if (ticket !== generation) throw new Error("DANDELION_CANCELLED");
      if (action === "status") {
        if (!stateDirectory) throw new Error("STATE_DISABLED");
        return execute(ticket, ["status", "--directory", stateDirectory, "--json"]);
      }
      return execute(ticket, [action === "selfTest" ? "self-test" : "doctor", "--json"]);
    },
    async compat(action, args) {
      const ticket = generation;
      await requireNative();
      if (ticket !== generation) throw new Error("DANDELION_CANCELLED");
      if (!compatDirectory) throw new Error("COMPAT_DISABLED");
      if (action === "packetInspect") return execute(ticket, ["compat", "packet-inspect", "--file", await compatPath(args.file, "packet.bin"), "--json"]);
      if (action === "countryLookup") {
        const address = typeof args.address === "string" ? args.address : null;
        if (!address) throw new Error("COMPAT_ADDRESS_REQUIRED");
        return execute(ticket, ["compat", "country-lookup", "--locations", await compatPath(args.locations, "locations.csv"), "--blocks", await compatPath(args.blocks, "blocks.csv"), "--address", address, "--json"]);
      }
      if (action === "linkExercise") {
        return execute(ticket, ["compat", "link-exercise", "--file", await compatPath(args.file, "link-target"), "--cycles", "10", "--interval-ms", "50", "--confirm", "--json"]);
      }
      return execute(ticket, ["compat", "link-recover", "--journal-directory", await compatPath(args.journalDirectory, "."), "--confirm", "--json"]);
    },
  };
}
