#!/usr/bin/env python3
"""Run only on an ephemeral CI runner. Local fixtures and loopback traffic only."""
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time
from disk_guard import check

parser = argparse.ArgumentParser()
parser.add_argument("--binary", type=Path, required=True)
args = parser.parse_args()
binary = str(args.binary.resolve())
check(Path(tempfile.gettempdir()), 32 * 1024**2)
env = {k: v for k, v in os.environ.items() if not k.startswith("DANDELION_")}
results = {}


def command(*args):
    return subprocess.run([binary, *args], env=env, capture_output=True, text=True, timeout=15, check=True)


results["self_test"] = json.loads(command("self-test", "--json").stdout)
assert results["self_test"]["verified_payload_bytes"] == 65537
with tempfile.TemporaryDirectory(prefix="dandelion-acceptance-") as directory:
    root = Path(directory)
    (root / "payload").write_bytes(bytes(range(256)) * 32768)
    original = (root / "payload").read_bytes()
    proc = subprocess.Popen([binary, "run", "--source", str(root), "--rate-bytes-per-second", "1",
                             "--queue-capacity", "1", "--json"], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
        time.sleep(0.4)
        assert proc.poll() is None, "rate limiter or process unexpectedly stopped"
        status = Path(f"/proc/{proc.pid}/status").read_text()
        rss_kib = int(next(line for line in status.splitlines() if line.startswith("VmRSS:")).split()[1])
        assert rss_kib < 128 * 1024, f"stalled pipeline RSS regression: {rss_kib} KiB"
        started = time.monotonic()
        proc.send_signal(signal.SIGTERM)
        stdout, stderr = proc.communicate(timeout=4)
        duration = time.monotonic() - started
        assert proc.returncode == 130, (proc.returncode, stdout, stderr)
        snapshot = json.loads(stdout)
        assert snapshot["status"] == "cancelled"
        assert duration < 3
        results["sigterm_under_backpressure"] = {"exit_code": proc.returncode, "shutdown_seconds": duration,
                                                 "rss_kib": rss_kib, "snapshot": snapshot}
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.communicate(timeout=3)
    assert (root / "payload").read_bytes() == original, "source files must remain unchanged"
    assert sorted(p.name for p in root.iterdir()) == ["payload"]
    results["source_unchanged"] = True
print(json.dumps(results, indent=2))
