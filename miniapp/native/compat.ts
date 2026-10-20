/** Real compatibility operations on explicitly picked, bounded private copies. */
import { execFileSync } from "node:child_process";
import { constants, closeSync, fstatSync, mkdtempSync, openSync, readSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { isIP } from "node:net";
import { join, relative, isAbsolute } from "node:path";
import { reserve } from "./profile.js";

export function createCompatibility(binary: string, temporaryRoot: string, host: (method: string, args: Record<string, unknown>) => Promise<unknown>) {
  let lastLink: { job: string; workspace: string } | null = null;
  const execute = (args: string[]) => JSON.parse(execFileSync(binary, ["compat", ...args, "--json"], { encoding: "utf8", timeout: 30000, maxBuffer: 1024 * 1024, stdio: ["ignore", "pipe", "pipe"] }));
  const copyPicked = async (destination: string, maxBytes: number) => {
    const picked = await host("file-picker", { mode: "files" }) as { path: string }[] | null;
    if (!picked?.length) throw new Error("SELECTION_CANCELLED");
    const fd = openSync(picked[0]!.path, constants.O_RDONLY | constants.O_NOFOLLOW);
    try {
      const before = fstatSync(fd, { bigint: true });
      if (!before.isFile() || before.size > BigInt(maxBytes)) throw new Error("COMPAT_FILE_LIMIT");
      reserve(temporaryRoot, Number(before.size) * 2 + 1048576);
      const bytes = Buffer.alloc(Number(before.size) + 1);
      let length = 0, n: number;
      while (length < bytes.length && (n = readSync(fd, bytes, length, bytes.length - length, length)) > 0) length += n;
      const after = fstatSync(fd, { bigint: true });
      if (length !== Number(before.size) || before.mtimeNs !== after.mtimeNs || before.ctimeNs !== after.ctimeNs) throw new Error("COMPAT_FILE_CHANGED");
      writeFileSync(destination, bytes.subarray(0, length), { flag: "wx", mode: 0o600 });
    } finally { closeSync(fd); }
  };
  return async (action: string, args: unknown) => {
    reserve(temporaryRoot, 1048576);
    if (action === "linkRecover") {
      if (!lastLink || await host("confirmation", { detail: "恢复本小程序私有副本上次链接练习的日志；不修改原文件。" }) !== true) throw new Error("NO_OWNED_JOURNAL");
      return execute(["link-recover", "--journal-directory", lastLink.workspace, "--confirm"]);
    }
    const job = mkdtempSync(join(temporaryRoot, "compat-job-"));
    let retain = false;
    try {
      if (action === "packetInspect") {
        const file = join(job, "packet.bin"); await copyPicked(file, 60020);
        return execute(["packet-inspect", "--file", file]);
      }
      if (action === "countryLookup") {
        const address = args && typeof args === "object" ? Reflect.get(args, "address") : null;
        if (typeof address !== "string" || !isIP(address)) throw new Error("INVALID_ADDRESS");
        if (await host("confirmation", { detail: "依次选择地理位置 locations CSV 和网络 blocks CSV。只在本机私有临时目录查询，完成后删除副本。" }) !== true) throw new Error("NOT_APPROVED");
        const locations = join(job, "locations.csv"), blocks = join(job, "blocks.csv");
        await copyPicked(locations, 64 * 1024 * 1024); await copyPicked(blocks, 64 * 1024 * 1024);
        return execute(["country-lookup", "--locations", locations, "--blocks", blocks, "--address", address]);
      }
      if (action !== "linkExercise" || await host("confirmation", { detail: "选择一个文件，在小程序私有副本上进行 10 次链接练习；原文件不变，关闭小程序即清理副本。" }) !== true) throw new Error("NOT_APPROVED");
      const file = join(job, "link-target"); await copyPicked(file, 1024 * 1024);
      const result = execute(["link-exercise", "--file", file, "--cycles", "10", "--interval-ms", "50", "--confirm"]);
      const workspace = result.workspace;
      const canonicalJob = realpathSync(job);
      if (typeof workspace !== "string" || isAbsolute(relative(canonicalJob, workspace)) || relative(canonicalJob, workspace).startsWith("..")) throw new Error("JOURNAL_SCOPE_MISMATCH");
      if (lastLink) rmSync(lastLink.job, { recursive: true, force: true });
      lastLink = { job, workspace }; retain = true;
      return result;
    } finally { if (!retain) rmSync(job, { recursive: true, force: true }); }
  };
}
