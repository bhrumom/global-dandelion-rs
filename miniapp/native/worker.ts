/** Package-owned business composition. Fabushi only transports bounded messages. */
import { AsyncLocalStorage } from "node:async_hooks";
import { randomUUID } from "node:crypto";
import { join } from "node:path";
import { createDandelionCliBackend } from "./miniapp-cli-backend.js";
import { createMiniAppToolRouter, type MiniAppToolScope } from "./miniapp-tools.js";
import { createProfile, publicResult } from "./profile.js";
import { macObservation } from "./macos.js";

function main() {
  const context = new AsyncLocalStorage<string>();
  const hostCalls = new Map<string, { resolve(result: unknown): void; reject(error: Error): void; timer: ReturnType<typeof setTimeout> }>();
  let input = "", active = 0;
  const output = (message: unknown) => {
    const line = JSON.stringify(message);
    if (Buffer.byteLength(line) > 1024 * 1024) throw new Error("MESSAGE_LIMIT");
    process.stdout.write(line + "\n");
  };
  const host = (method: string, args: Record<string, unknown>) => new Promise<unknown>((resolve, reject) => {
    const id = context.getStore();
    if (!id || hostCalls.size >= 8) { reject(new Error("HOST_REQUEST_UNAVAILABLE")); return; }
    const callId = randomUUID();
    const timer = setTimeout(() => { hostCalls.delete(callId); reject(new Error("HOST_REQUEST_TIMEOUT")); }, 120000);
    hostCalls.set(callId, { resolve, reject, timer });
    output({ kind: "host", id, callId, method, ...args });
  });
  const dataDirectory = process.env.FABUSHI_NATIVE_DATA_DIRECTORY;
  if (!dataDirectory) throw new Error("PRIVATE_DATA_DIRECTORY_REQUIRED");
  const profile = createProfile(join(__dirname, "dandelion.bin"), join(__dirname, "default.toml"), dataDirectory);
  const ports = {
    pick: async mode => {
      const result = await host("file-picker", { mode });
      if (result === null) return null;
      if (!Array.isArray(result) || result.length > 1 || result.some(p => !p || typeof p.path !== "string" || typeof p.name !== "string" || typeof p.size !== "number")) throw new Error("INVALID_PICKER_RESULT");
      return result;
    },
  } satisfies Parameters<typeof createDandelionCliBackend>[1];
  let backend = createDandelionCliBackend(profile.options(), ports);
  const makeRouter = () => createMiniAppToolRouter({ backend, allowedAppId: "org.fabushi.global-dandelion", approve: async tool => await host("confirmation", { detail: tool }) === true });
  let router = makeRouter();
  const invokePackage = async (scope: MiniAppToolScope, method: unknown, args: unknown) => {
    const denied = { ok: false, error: { code: "PACKAGE_TOOL_FAILED", message: "操作未完成，请检查本机配置或重新授权。" } };
    if (scope.appId !== "org.fabushi.global-dandelion") return denied;
    if (method === "update.check" || method === "update.stage") {
      try {
        if (method === "update.stage" && await host("confirmation", { detail: "安装本小程序已审核的更新；旧代码和运行任务将关闭，需重新打开。" }) !== true) return denied;
        return { ok: true, data: await host("package-update", { action: method === "update.check" ? "check" : "stage" }) };
      } catch { return denied; }
    }
    if (method === "config.import") {
      let candidate: ReturnType<typeof profile.prepareImport> | undefined;
      try {
        const picked = await host("file-picker", { mode: "files" }) as { path: string }[] | null;
        if (!picked?.length) return denied;
        candidate = profile.prepareImport(picked[0]!.path);
        if (!await host("confirmation", { detail: "导入配置并授权其中的证书和目标：" + candidate.summary.slice(0, 180) })) return denied;
        candidate.commit(); backend.cancelAll?.();
        backend = createDandelionCliBackend(profile.options(), ports); router = makeRouter();
        return { ok: true, data: { imported: true, privateState: true, automaticSourcesRemoved: true } };
      } catch { return denied; } finally { candidate?.discard(); }
    }
    if (typeof method === "string" && /^platform\.(inspect|mounts|time|watch)$/.test(method)) {
      try { return { ok: true, data: await macObservation(method.split(".")[1]!) }; } catch { return denied; }
    }
    if (method === "config.check" && profile.error()) return { ok: false, error: { code: "CONFIG_INVALID", message: profile.error()! } };
    const result = await router.invoke(scope, method, args);
    if (method === "offline.capabilities" && result.ok && result.data && typeof result.data === "object") {
      const data = result.data as { capabilities: string[] };
      if (process.platform === "darwin" && !data.capabilities.includes("platform")) data.capabilities.push("platform");
      if (!data.capabilities.includes("update")) data.capabilities.push("update");
    }
    return publicResult(result);
  };
  const shutdown = () => {
    backend.cancelAll?.();
    for (const call of hostCalls.values()) { clearTimeout(call.timer); call.reject(new Error("RUNTIME_CLOSED")); }
    hostCalls.clear();
  };
  process.once("SIGTERM", () => { shutdown(); process.exit(0); });
  process.once("SIGINT", () => { shutdown(); process.exit(0); });
  process.stdin.on("end", () => { shutdown(); process.exit(0); });
  const handle = async (line: string) => {
    const message = JSON.parse(line) as Record<string, unknown>;
    if (message.kind === "host-result" && typeof message.callId === "string") {
      const pending = hostCalls.get(message.callId);
      if (!pending) throw new Error("UNKNOWN_HOST_RESPONSE");
      clearTimeout(pending.timer); hostCalls.delete(message.callId);
      if (message.error) pending.reject(new Error("HOST_OPERATION_FAILED"));
      else pending.resolve(message.result);
      return;
    }
    if (message.kind !== "invoke" || typeof message.id !== "string" || active >= 8 || !message.scope || typeof message.scope !== "object") throw new Error("INVALID_INVOCATION");
    active++;
    try {
      await context.run(message.id, async () => {
        const result = await invokePackage(message.scope as MiniAppToolScope, message.method, message.args);
        output({ kind: "result", id: message.id, result });
      });
    } finally { active--; }
  };
  process.stdin.setEncoding("utf8");
  process.stdin.on("data", (chunk: string) => {
    input += chunk;
    if (Buffer.byteLength(input) > 1024 * 1024) { shutdown(); process.exit(1); }
    let end: number;
    while ((end = input.indexOf("\n")) >= 0) {
      const line = input.slice(0, end); input = input.slice(end + 1);
      void handle(line).catch(() => { shutdown(); process.exit(1); });
    }
  });
}
main();
