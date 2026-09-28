#!/usr/bin/env python3
"""Controlled same-host Python/Rust cutover harness.

This script NEVER imports or executes the pinned legacy module. It verifies the exact
Git blob first, then runs a quarantined helper that reproduces only reviewed,
side-effect-free scan/read/legacy-loopback expressions on synthetic local fixtures.
"""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import signal
import socket
import statistics
import struct
import subprocess
import sys
import tempfile
import threading
import time

SOURCE_COMMIT = "aa7f3720c42856f0e1d18742f78fba156227ee79"
SOURCE_BLOB = "54bf6226eb80d1ef6c4f6087fb02579141ab134f"

REQUIRED_SNIPPETS = (
    "for root, dirs, files in os.walk(directory, topdown=True):",
    "dirs[:] = [d for d in dirs if not d.startswith('.') and d != '__pycache__']",
    "packet = struct.pack('!I', chunk_index) + hashlib.md5(chunk_data).digest() + chunk_data",
)


def git_blob_sha1(raw: bytes) -> str:
    return hashlib.sha1(b"blob " + str(len(raw)).encode() + b"\0" + raw).hexdigest()


def verify_source(path: Path) -> None:
    raw = path.read_bytes()
    if git_blob_sha1(raw) != SOURCE_BLOB:
        raise RuntimeError("legacy source blob does not match the pinned baseline")
    text = raw.decode("utf-8")
    missing = [snippet for snippet in REQUIRED_SNIPPETS if snippet not in text]
    if missing:
        raise RuntimeError(f"pinned source is missing reviewed expressions: {missing}")


def iter_files(root: Path):
    for directory, dirs, files in os.walk(root, topdown=True):
        dirs[:] = [d for d in dirs if not d.startswith(".") and d != "__pycache__"]
        for name in files:
            if not name.startswith("."):
                yield Path(directory) / name


def worker_metadata(root: Path) -> dict:
    files = 0
    directories = 1
    total = 0
    for directory, dirs, names in os.walk(root, topdown=True):
        dirs[:] = [d for d in dirs if not d.startswith(".") and d != "__pycache__"]
        directories += len(dirs)
        for name in names:
            if name.startswith("."):
                continue
            path = Path(directory) / name
            stat = path.stat()
            if path.is_file():
                files += 1
                total += stat.st_size
    return {
        "status": "completed",
        "regular_files": files,
        "directories": directories,
        "total_file_bytes": total,
        "payload_bytes_read": 0,
    }


def worker_read(root: Path, chunk_bytes: int) -> dict:
    total = 0
    files = 0
    digest = hashlib.sha256()
    for path in iter_files(root):
        with path.open("rb") as stream:
            while True:
                chunk = stream.read(chunk_bytes)
                if not chunk:
                    break
                total += len(chunk)
                digest.update(chunk)
        files += 1
    return {
        "status": "completed",
        "files_completed": files,
        "verified_payload_bytes": total,
        "sha256": digest.hexdigest(),
    }


def worker_transfer(root: Path, chunk_bytes: int) -> dict:
    receiver = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    receiver.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 4 * 1024 * 1024)
    receiver.bind(("127.0.0.1", 0))
    receiver.settimeout(0.05)
    target = receiver.getsockname()
    stop = threading.Event()
    received = {"wire": 0, "payload": 0, "packets": 0, "bad_md5": 0}

    def recv_loop():
        while not stop.is_set():
            try:
                data, _ = receiver.recvfrom(65536)
            except socket.timeout:
                continue
            except OSError:
                break
            if len(data) < 20:
                received["bad_md5"] += 1
                continue
            payload = data[20:]
            if hashlib.md5(payload).digest() != data[4:20]:
                received["bad_md5"] += 1
                continue
            received["wire"] += len(data)
            received["payload"] += len(payload)
            received["packets"] += 1

    thread = threading.Thread(target=recv_loop, daemon=True)
    thread.start()
    sender = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sender.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 4 * 1024 * 1024)
    attempted_wire = 0
    payload_bytes = 0
    packets = 0
    try:
        for path in iter_files(root):
            with path.open("rb") as stream:
                chunk_index = 0
                while True:
                    chunk = stream.read(chunk_bytes)
                    if not chunk:
                        break
                    packet = struct.pack("!I", chunk_index) + hashlib.md5(chunk).digest() + chunk
                    sender.sendto(packet, target)
                    attempted_wire += len(packet)
                    payload_bytes += len(chunk)
                    packets += 1
                    chunk_index += 1
        deadline = time.monotonic() + 1.0
        while received["packets"] < packets and time.monotonic() < deadline:
            time.sleep(0.005)
    finally:
        stop.set()
        sender.close()
        receiver.close()
        thread.join(timeout=1)
    return {
        "status": "completed",
        "files_completed": sum(1 for _ in iter_files(root)),
        "chunks_sent": packets,
        "attempted_wire_bytes": attempted_wire,
        "payload_bytes": payload_bytes,
        "receiver_wire_bytes": received["wire"],
        "receiver_payload_bytes": received["payload"],
        "receiver_packets": received["packets"],
        "receiver_bad_md5": received["bad_md5"],
        "wire_contract": "legacy u32be + MD5 + payload; receiver integrity check added by cutover harness",
    }


def worker_shutdown(root: Path, chunk_bytes: int) -> dict:
    stopping = threading.Event()
    signal.signal(signal.SIGTERM, lambda *_: stopping.set())
    total = 0
    while not stopping.is_set():
        for path in iter_files(root):
            with path.open("rb") as stream:
                while not stopping.is_set():
                    chunk = stream.read(chunk_bytes)
                    if not chunk:
                        break
                    total += len(chunk)
                    time.sleep(0.001)
            if stopping.is_set():
                break
    return {"status": "cancelled", "bytes_read_before_signal_settlement": total}


def percentile(values, p: float):
    ordered = sorted(values)
    if not ordered:
        return None
    index = min(len(ordered) - 1, max(0, math.ceil(p * len(ordered)) - 1))
    return ordered[index]


def timed(command: list[str], env: dict[str, str], timeout: int = 120) -> dict:
    with tempfile.NamedTemporaryFile(prefix="gdan-time-", suffix=".json", delete=False) as handle:
        metrics = Path(handle.name)
    try:
        fmt = '{"user_seconds":%U,"system_seconds":%S,"peak_rss_kib":%M,"wall_seconds":%e}'
        proc = subprocess.run(
            ["/usr/bin/time", "-f", fmt, "-o", str(metrics), *command],
            capture_output=True, text=True, env=env, timeout=timeout, check=True
        )
        result = json.loads(proc.stdout)
        result["process_metrics"] = json.loads(metrics.read_text())
        return result
    finally:
        metrics.unlink(missing_ok=True)


def shutdown_sample(command: list[str], env: dict[str, str]) -> dict:
    proc = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env)
    time.sleep(0.25)
    start = time.monotonic()
    proc.send_signal(signal.SIGTERM)
    stdout, stderr = proc.communicate(timeout=6)
    elapsed = time.monotonic() - start
    if proc.returncode not in (0, 130):
        raise RuntimeError(f"shutdown command failed: {proc.returncode}: {stderr}")
    parsed = json.loads(stdout)
    return {"exit_code": proc.returncode, "shutdown_seconds": elapsed, "result": parsed}


def build_fixture(root: Path, large_mib: int, small_files: int) -> dict:
    groups = [root / "a", root / "b" / "c", root / "d" / "e" / "f"]
    for directory in groups:
        directory.mkdir(parents=True, exist_ok=True)
    for index in range(small_files):
        size = ((index % 17) + 1) * 4096
        (groups[index % len(groups)] / f"f{index:05d}.bin").write_bytes(bytes([index % 251]) * size)
    large = root / "large.bin"
    block = bytes(range(256)) * 4096
    with large.open("wb") as stream:
        for _ in range(large_mib):
            stream.write(block)
    (root / ".hidden").write_bytes(b"excluded")
    (root / "__pycache__").mkdir()
    (root / "__pycache__" / "excluded.pyc").write_bytes(b"excluded")
    files = list(iter_files(root))
    total = sum(path.stat().st_size for path in files)
    return {"regular_files": len(files), "total_file_bytes": total}


def summarize(samples: list[dict], payload_bytes: int) -> dict:
    wall = [s["process_metrics"]["wall_seconds"] for s in samples]
    cpu = [s["process_metrics"]["user_seconds"] + s["process_metrics"]["system_seconds"] for s in samples]
    rss = [s["process_metrics"]["peak_rss_kib"] for s in samples]
    gib = payload_bytes / (1024 ** 3)
    return {
        "samples": len(samples),
        "wall_seconds_median": statistics.median(wall),
        "wall_seconds_p95": percentile(wall, 0.95),
        "cpu_seconds_median": statistics.median(cpu),
        "cpu_seconds_per_dataset_gib_median": statistics.median(cpu) / gib if gib else None,
        "peak_rss_kib_max": max(rss),
        "dataset_throughput_mib_per_second_median": (payload_bytes / 1024 ** 2) / statistics.median(wall) if statistics.median(wall) else None,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--python-source", type=Path)
    parser.add_argument("--tested-commit")
    parser.add_argument("--artifact-head-commit")
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--large-mib", type=int, default=64)
    parser.add_argument("--small-files", type=int, default=1000)
    parser.add_argument("--chunk-bytes", type=int, default=1024)
    parser.add_argument("--worker", choices=["metadata", "read", "transfer", "shutdown"])
    parser.add_argument("--root", type=Path)
    args = parser.parse_args()

    if args.worker:
        if not args.root:
            raise SystemExit("--root is required with --worker")
        result = {
            "metadata": worker_metadata,
            "read": lambda r: worker_read(r, args.chunk_bytes),
            "transfer": lambda r: worker_transfer(r, args.chunk_bytes),
            "shutdown": lambda r: worker_shutdown(r, args.chunk_bytes),
        }[args.worker](args.root)
        print(json.dumps(result))
        return

    if not args.binary or not args.python_source:
        raise SystemExit("--binary and --python-source are required")
    if args.repeats < 3:
        raise SystemExit("--repeats must be at least 3")
    if not args.tested_commit or not args.artifact_head_commit:
        raise SystemExit("--tested-commit and --artifact-head-commit are required")
    if len(args.tested_commit) != 40 or any(ch not in "0123456789abcdef" for ch in args.tested_commit.lower()):
        raise SystemExit("--tested-commit must be a full 40-hex commit SHA")
    if args.artifact_head_commit != args.tested_commit:
        raise SystemExit("artifact workflow head_sha must equal the exact tested commit")
    verify_source(args.python_source)
    binary = str(args.binary.resolve())
    env = {k: v for k, v in os.environ.items() if not k.startswith("DANDELION_")}

    with tempfile.TemporaryDirectory(prefix="gdan-controlled-cutover-") as tmp:
        root = Path(tmp) / "fixture"
        fixture = build_fixture(root, args.large_mib, args.small_files)
        payload = fixture["total_file_bytes"]
        samples = {"metadata": {"python": [], "rust": []}, "read": {"python": [], "rust": []}, "transfer": {"python": [], "rust": []}}
        script = str(Path(__file__).resolve())
        py = lambda worker: [sys.executable, script, "--worker", worker, "--root", str(root), "--chunk-bytes", str(args.chunk_bytes)]
        rust = {
            "metadata": [binary, "scan", "metadata", "--source", str(root), "--json"],
            "read": [binary, "scan", "read", "--source", str(root), "--json", "--rate-bytes-per-second", "1000000000"],
            "transfer": [binary, "run", "--source", str(root), "--json", "--rate-bytes-per-second", "1000000000",
                         "--traffic-limit-bytes", str(max(payload * 4, 64 * 1024 * 1024)), "--queue-capacity", "32",
                         "--chunk-bytes", str(args.chunk_bytes)],
        }
        for workload in ("metadata", "read", "transfer"):
            for repeat in range(args.repeats):
                order = ("python", "rust") if repeat % 2 == 0 else ("rust", "python")
                for language in order:
                    command = py(workload) if language == "python" else rust[workload]
                    samples[workload][language].append(timed(command, env))

        py_meta = samples["metadata"]["python"][0]
        rs_meta = samples["metadata"]["rust"][0]
        if py_meta["regular_files"] != fixture["regular_files"] or rs_meta["regular_files"] != fixture["regular_files"]:
            raise RuntimeError("metadata workload did not cover the same fixture")
        for language in ("python", "rust"):
            for sample in samples["read"][language]:
                if sample["verified_payload_bytes"] != payload:
                    raise RuntimeError(f"{language} read workload did not cover the complete payload")
        for sample in samples["transfer"]["rust"]:
            if sample["verified_payload_bytes"] != payload:
                raise RuntimeError("Rust transfer did not verify the complete payload")
        if any(sample["receiver_bad_md5"] for sample in samples["transfer"]["python"]):
            raise RuntimeError("Python legacy transfer receiver observed an MD5 mismatch")

        shutdown = {"python": [], "rust": []}
        for repeat in range(args.repeats):
            shutdown["python"].append(shutdown_sample(py("shutdown"), env))
            shutdown["rust"].append(shutdown_sample(
                [binary, "run", "--source", str(root), "--rate-bytes-per-second", "1",
                 "--traffic-limit-bytes", str(max(payload * 4, 64 * 1024 * 1024)), "--queue-capacity", "1", "--json"], env
            ))

        report = {
            "schema_version": 1,
            "gate_id": "controlled-cutover",
            "tested_commit": args.tested_commit,
            "artifact_workflow_head_commit": args.artifact_head_commit,
            "result": "pass",
            "performed_by": f"controlled-cutover.py on {socket.gethostname()}",
            "python_source_commit": SOURCE_COMMIT,
            "python_source_blob": SOURCE_BLOB,
            "source_execution_policy": "legacy module hash/snippets verified; legacy main never imported or executed",
            "fixture": fixture,
            "repeats": args.repeats,
            "hardware": {
                "machine": os.uname().machine,
                "kernel": os.uname().release,
                "cpu_count": os.cpu_count(),
            },
            "workloads": {
                workload: {
                    language: {"raw": rows, "summary": summarize(rows, payload)}
                    for language, rows in languages.items()
                }
                for workload, languages in samples.items()
            },
            "shutdown": {
                language: {
                    "raw": rows,
                    "median_seconds": statistics.median(r["shutdown_seconds"] for r in rows),
                    "p95_seconds": percentile([r["shutdown_seconds"] for r in rows], 0.95),
                }
                for language, rows in shutdown.items()
            },
            "interpretation": {
                "metadata": "same synthetic tree; payload is not read by either metadata workload",
                "read": "same complete payload bytes; protocols are not involved",
                "transfer": "Python uses pinned legacy u32be+MD5 UDP framing and records receiver loss; Rust uses protocol v2 with verified local receipts. Wire contracts differ and are not treated as identical work.",
                "performance_claim": "measurement only; pass means the controlled workloads completed and integrity/scope invariants held, not that Rust is universally faster",
            },
            "review_status": "candidate evidence only until an independent reviewer validates this report and supplies reviewed_by in the external evidence manifest",
        }
        print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
