#!/usr/bin/env python3
"""Package raw Criterion evidence; never infer Python speedup from packet v2."""
import json
import os
from pathlib import Path
import platform

root = Path("target/criterion")
measurements = {}
for path in sorted(root.glob("**/new/estimates.json")):
    name = str(path.parent.parent.relative_to(root))
    if name.startswith("packet-v2/"):
        measurements[name] = json.loads(path.read_text())
if len(measurements) != 2:
    raise RuntimeError(f"expected encode/decode evidence, found {list(measurements)}")
print(json.dumps({
    "schema_version": 1,
    "git_sha": os.environ.get("GITHUB_SHA"),
    "kernel": platform.release(),
    "architecture": platform.machine(),
    "cpu_count": os.cpu_count(),
    "measurement_units": "nanoseconds per packet operation",
    "payload_bytes": 1152,
    "scope": "shared CI runner microbenchmark; not a Python comparison or release performance guarantee",
    "estimates": measurements,
}, indent=2))
