#!/usr/bin/env python3
"""Synthetic negative/positive checks for the external evidence verifier."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
from check_parity import ROOT, validate_external_evidence

gates = json.loads((ROOT / "docs/acceptance-gates.json").read_text())
current = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()

def expect_failure(path: Path, message: str) -> None:
    try:
        validate_external_evidence(path, current, gates)
    except RuntimeError:
        return
    raise RuntimeError(f"evidence verifier accepted invalid case: {message}")

with tempfile.TemporaryDirectory(prefix="dandelion-evidence-contract-") as directory:
    root = Path(directory)
    proof = {"schema_version": gates["evidence_contract"]["schema_version"], "tested_commit": current, "gates": {}}
    report_paths = {}
    for gate in gates["required_gates"]:
        gate_id = gate["id"]
        report = {"schema_version": 1, "gate_id": gate_id, "tested_commit": current, "result": "pass", "performed_by": f"synthetic-executor:{gate_id}"}
        path = root / f"{gate_id}.json"
        raw = (json.dumps(report, sort_keys=True) + "\n").encode()
        path.write_bytes(raw)
        report_paths[gate_id] = path
        proof["gates"][gate_id] = {"gate_id": gate_id, "result": "pass", "report": path.name, "report_sha256": hashlib.sha256(raw).hexdigest(), "reviewed_by": f"synthetic-reviewer:{gate_id}"}
    manifest = root / "manifest.json"
    manifest.write_text(json.dumps(proof, indent=2) + "\n")
    validate_external_evidence(manifest, current, gates)
    first = gates["required_gates"][0]["id"]
    saved = report_paths[first].read_bytes()
    report_paths[first].write_bytes(saved + b" ")
    expect_failure(manifest, "tampered report with stale digest")
    report_paths[first].write_bytes(saved)
    self_review = copy.deepcopy(proof)
    self_review["gates"][first]["reviewed_by"] = f"synthetic-executor:{first}"
    manifest.write_text(json.dumps(self_review, indent=2) + "\n")
    expect_failure(manifest, "performer reviewed own gate")
    missing_report = copy.deepcopy(proof)
    del missing_report["gates"][first]["report"]
    manifest.write_text(json.dumps(missing_report, indent=2) + "\n")
    expect_failure(manifest, "manifest-only pass without report")
    wrong_gate = copy.deepcopy(proof)
    wrong_gate["gates"][first]["gate_id"] = "wrong-gate"
    manifest.write_text(json.dumps(wrong_gate, indent=2) + "\n")
    expect_failure(manifest, "gate ID mismatch")
print(json.dumps({"evidence_contract": "pass", "synthetic_only": True}))
