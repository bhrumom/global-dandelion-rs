#!/usr/bin/env python3
"""Require a disposition, an honest implementation status and evidence paths."""
import json
from pathlib import Path
root = Path(__file__).resolve().parents[1]
data = json.loads((root / "docs/migration/parity.json").read_text())
assert data["source_commit"] == "aa7f3720c42856f0e1d18742f78fba156227ee79"
assert data["source_blob"] == "54bf6226eb80d1ef6c4f6087fb02579141ab134f"
seen = set()
for row in data["capabilities"]:
    assert row["id"] not in seen
    seen.add(row["id"])
    assert row["classification"] in {"mapped", "redesigned", "legacy-adapter", "intentionally-removed", "not-applicable"}
    assert row["status"] in {"implemented", "pending", "removed"}
    assert row["symbols"] and row["reason"]
    assert len(row["source_lines"]) == 2 and row["source_lines"][0] <= row["source_lines"][1]
    if row["status"] != "pending":
        assert row["evidence"], row["id"]
        for file in row["evidence"]:
            assert (root / file.split("#")[0]).is_file(), (row["id"], file)
assert len(seen) >= 32
print(json.dumps({"parity_ledger": "pass", "rows": len(seen),
                  "implemented": sum(r["status"] == "implemented" for r in data["capabilities"]),
                  "pending": sum(r["status"] == "pending" for r in data["capabilities"]),
                  "full_migration_complete": False}))
