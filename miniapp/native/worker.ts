/** Package-owned business composition. Fabushi only transports bounded messages. */
import { AsyncLocalStorage } from "node:async_hooks";
import { randomUUID } from "node:crypto";
import { join } from "node:path";
import { createDandelionCliBackend } from "./miniapp-cli-backend.js";
import { createMiniAppToolRouter, type MiniAppToolScope } from "./miniapp-tools.js";

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
  const backend = createDandelionCliBackend({
    binaryPath: join(__dirname, "dandelion.bin"),
    configPath: join(__dirname, "default.toml"),
    allowedRoots: [], authorizePickedRoots: true,
    targetPools: [{ id: "loopback", label: "本机回环验证" }],
  }, {
    pick: async mode => {
      const result = await host("file-picker", { mode });
      if (result === null) return null;
      if (!Array.isArray(result) || result.length > 1 || result.some(p => !p || typeof p.path !== "string" || typeof p.name !== "string" || typeof p.size !== "number")) throw new Error("INVALID_PICKER_RESULT");
      return result;
    },
  });
  const router = createMiniAppToolRouter({ backend, allowedAppId: "org.fabushi.global-dandelion", approve: async tool => await host("confirmation", { detail: tool }) === true });
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
      clearTimeout(pending.timer); hostCalls.delete(message.callId); pending.resolve(message.result);
      return;
    }
    if (message.kind !== "invoke" || typeof message.id !== "string" || active >= 8 || !message.scope || typeof message.scope !== "object") throw new Error("INVALID_INVOCATION");
    active++;
    try {
      await context.run(message.id, async () => {
        const result = await router.invoke(message.scope as MiniAppToolScope, message.method, message.args);
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
