/** Package-owned read-only macOS observation. No Host feature or source grant. */
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { arch, cpus, freemem, networkInterfaces, release, totalmem, uptime } from "node:os";
import { basename } from "node:path";

function volumes() {
  const plist = execFileSync("/usr/sbin/diskutil", ["list", "-plist"], { timeout: 15000, maxBuffer: 1024 * 1024, stdio: ["ignore", "pipe", "pipe"] });
  const raw = execFileSync("/usr/bin/plutil", ["-convert", "json", "-o", "-", "-"], { input: plist, encoding: "utf8", timeout: 15000, maxBuffer: 1024 * 1024, stdio: ["pipe", "pipe", "pipe"] });
  const result: { id: string; label: string; filesystem: string; sizeBytes: number }[] = [];
  const seen = new Set<string>();
  let visited = 0;
  function walk(value: unknown, depth = 0) {
    if (++visited > 4096 || depth > 24) throw new Error("INVENTORY_LIMIT");
    if (Array.isArray(value)) { for (const child of value) walk(child, depth + 1); return; }
    if (!value || typeof value !== "object") return;
    const item = value as Record<string, unknown>;
    if (typeof item.MountPoint === "string" && typeof item.DeviceIdentifier === "string" && !seen.has(item.MountPoint)) {
      if (result.length >= 256) throw new Error("INVENTORY_LIMIT");
      seen.add(item.MountPoint);
      result.push({ id: createHash("sha256").update(item.DeviceIdentifier).update(item.MountPoint).digest("hex").slice(0, 24), label: typeof item.VolumeName === "string" ? item.VolumeName : basename(item.MountPoint) || "系统卷", filesystem: typeof item.Content === "string" ? item.Content : "unknown", sizeBytes: typeof item.Size === "number" && Number.isSafeInteger(item.Size) ? item.Size : 0 });
    }
    for (const child of Object.values(item)) if (child && typeof child === "object") walk(child, depth + 1);
  }
  walk(JSON.parse(raw));
  return result;
}
export async function macObservation(action: string) {
  if (process.platform !== "darwin") throw new Error("PLATFORM_UNSUPPORTED");
  if (action === "time") return { unix_seconds: Math.floor(Date.now() / 1000), uptime_seconds: uptime(), system_clock_modified: false, external_reference: false };
  if (action === "mounts") return { observationOnly: true, volumes: volumes() };
  if (action === "watch") {
    const samples = [];
    for (let sample = 0; sample < 5; sample++) { samples.push({ sample, volumes: volumes() }); if (sample < 4) await new Promise(resolve => setTimeout(resolve, 1000)); }
    return { observationOnly: true, samples };
  }
  if (action !== "inspect") throw new Error("UNKNOWN_PLATFORM_ACTION");
  return { observationOnly: true, platform: "macOS", release: release(), architecture: arch(), cpuCount: cpus().length, totalMemoryBytes: totalmem(), freeMemoryBytes: freemem(), interfaces: Object.entries(networkInterfaces()).slice(0, 64).map(([name, addresses]) => ({ name, addresses: (addresses ?? []).slice(0, 32).map(address => ({ address: address.address, family: address.family, internal: address.internal })) })), volumes: volumes() };
}
