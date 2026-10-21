import { StringDecoder } from "node:string_decoder";

/** A watch can run indefinitely; retained output cannot grow with its duration. */
export class ProgressStream {
  private readonly decoder = new StringDecoder("utf8");
  private pending = "";
  private latest = "";
  private terminal = "";
  push(chunk: Buffer): void {
    this.pending += this.decoder.write(chunk);
    let end: number;
    while ((end = this.pending.indexOf("\n")) >= 0) {
      const line = this.pending.slice(0, end).trim();
      this.pending = this.pending.slice(end + 1);
      if (!line || Buffer.byteLength(line) > 1_048_576 || this.terminal) throw new Error("INVALID_PROGRESS_STREAM");
      const value = JSON.parse(line);
      if (value?.event === "progress") {
        if (Buffer.byteLength(line) > 4096 || ![value.sequence, value.pass_verified_payload_bytes, value.pass_chunks_sent].every(v => Number.isSafeInteger(v) && v >= 0)) throw new Error("INVALID_PROGRESS_FRAME");
        this.latest = line + "\n";
      } else {
        if (value?.schema_version !== 1) throw new Error("INVALID_PROGRESS_RECEIPT");
        this.terminal = line + "\n";
      }
    }
    if (Buffer.byteLength(this.pending) > 1_048_576) throw new Error("PROGRESS_FRAME_LIMIT");
  }
  snapshot(): string { return this.latest + this.terminal + this.pending; }
}
