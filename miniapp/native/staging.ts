/** Bounded private snapshots of explicitly picked files; never widen to a parent. */
import { constants, type BigIntStats } from 'node:fs';
import { lstat, open, mkdtemp, rm } from 'node:fs/promises';
import { basename, isAbsolute, join } from 'node:path';
import { reserve } from './profile.js';

export async function stageFiles(paths: readonly string[], directory: string, limits: { maxFiles: number; maxBytes: number }, text = '') {
  const privateInfo = await lstat(directory);
  if (!privateInfo.isDirectory() || privateInfo.isSymbolicLink() || (privateInfo.mode & 0o077) !== 0) throw new Error('PRIVATE_STAGING_REQUIRED');
  const message = Buffer.from(text, 'utf8');
  if (message.length > 65536 || paths.length + (message.length ? 1 : 0) > Math.min(limits.maxFiles, 128)) throw new Error('SELECTION_LIMIT_EXCEEDED');
  const handles: { handle: Awaited<ReturnType<typeof open>>; stamp: BigIntStats; name: string }[] = [];
  let root: string | null = null;
  try {
    let total = message.length;
    for (const path of paths) {
      if (!isAbsolute(path)) throw new Error('ABSOLUTE_FILE_REQUIRED');
      const before = await lstat(path, { bigint: true });
      if (!before.isFile() || before.isSymbolicLink()) throw new Error('REGULAR_FILE_REQUIRED');
      const handle = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
      const stamp = await handle.stat({ bigint: true });
      handles.push({ handle, stamp, name: basename(path) });
      if (!stamp.isFile() || stamp.dev !== before.dev || stamp.ino !== before.ino) throw new Error('SOURCE_CHANGED');
      total += Number(stamp.size);
      if (!Number.isSafeInteger(total) || total > Math.min(limits.maxBytes, 1024 ** 3)) throw new Error('SELECTION_LIMIT_EXCEEDED');
    }
    reserve(directory, total + 2 * 1024 * 1024);
    root = await mkdtemp(join(directory, 'selection-'));
    const entries: { name: string; size: number }[] = [];
    const buffer = Buffer.alloc(65536);
    for (const [index, source] of handles.entries()) {
      const destination = await open(join(root, `file-${index}.bin`), 'wx', 0o600);
      try {
        let read = 0;
        while (read < Number(source.stamp.size)) {
          const { bytesRead } = await source.handle.read(buffer, 0, Math.min(buffer.length, Number(source.stamp.size) - read), read);
          if (!bytesRead) throw new Error('SOURCE_CHANGED');
          let written = 0;
          while (written < bytesRead) {
            const result = await destination.write(buffer, written, bytesRead - written);
            if (!result.bytesWritten) throw new Error('WRITE_STALLED');
            written += result.bytesWritten;
          }
          read += bytesRead;
        }
        const after = await source.handle.stat({ bigint: true });
        if (after.size !== source.stamp.size || after.mtimeNs !== source.stamp.mtimeNs || after.ctimeNs !== source.stamp.ctimeNs) throw new Error('SOURCE_CHANGED');
      } finally { await destination.close(); }
      entries.push({ name: source.name, size: Number(source.stamp.size) });
    }
    if (message.length) {
      const destination = await open(join(root, 'message.txt'), 'wx', 0o600);
      try { await destination.writeFile(message); } finally { await destination.close(); }
      entries.push({ name: '文字.txt', size: message.length });
    }
    const owned = root;
    return { root: owned, entries, dispose: () => rm(owned, { recursive: true, force: true }) };
  } catch (error) { if (root) await rm(root, { recursive: true, force: true }); throw error; }
  finally { await Promise.all(handles.map(source => source.handle.close())); }
}
