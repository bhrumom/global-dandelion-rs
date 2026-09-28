#!/usr/bin/env python3
"""Validate code coverage separately from externally reviewed full acceptance."""
import argparse
import hashlib
import hmac
import json
import os
from pathlib import Path
import re
import stat
import subprocess

ROOT = Path(__file__).resolve().parents[1]

def require(condition, message):
    if not condition:
        raise RuntimeError(message)

def read_bounded_regular(path: Path, limit: int, label: str) -> bytes:
    require(limit > 0, f"{label}: invalid size limit")
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        fd = os.open(path, flags)
    except OSError as error:
        raise RuntimeError(f"{label}: cannot open regular evidence file {path}: {error}") from error
    try:
        metadata = os.fstat(fd)
        require(stat.S_ISREG(metadata.st_mode), f"{label}: evidence must be a regular file")
        require(metadata.st_size <= limit, f"{label}: evidence exceeds {limit} bytes")
        with os.fdopen(fd, "rb", closefd=False) as stream:
            raw = stream.read(limit + 1)
        require(len(raw) <= limit, f"{label}: evidence grew past {limit} bytes")
        return raw
    finally:
        os.close(fd)

def load_json_bytes(raw: bytes, label: str):
    try:
        return json.loads(raw)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise RuntimeError(f"{label}: invalid JSON: {error}") from error

def resolve_report(manifest: Path, value: str) -> Path:
    require(isinstance(value, str) and value, "gate report path must be non-empty")
    report = Path(value)
    return report if report.is_absolute() else manifest.parent / report

def validate_external_evidence(evidence_path: Path, current: str, gates: dict) -> None:
    contract = gates.get("evidence_contract", {})
    require(contract.get("hash_algorithm") == "sha256", "unsupported evidence hash contract")
    manifest_limit = int(contract.get("max_manifest_bytes", 0))
    report_limit = int(contract.get("max_report_bytes", 0))
    raw = read_bounded_regular(evidence_path, manifest_limit, "evidence manifest")
    evidence = load_json_bytes(raw, "evidence manifest")
    require(evidence.get("schema_version") == contract.get("schema_version"), "evidence schema mismatch")
    require(evidence.get("tested_commit") == current, "Evidence must match the current checked-out revision")
    proofs = evidence.get("gates")
    require(isinstance(proofs, dict), "evidence gates must be an object")
    required_ids = {gate["id"] for gate in gates["required_gates"]}
    require(set(proofs) == required_ids, "evidence must contain exactly the required gate IDs")
    required_gate_fields = set(contract.get("required_gate_fields", []))
    required_report_fields = set(contract.get("required_report_fields", []))
    for gate in gates["required_gates"]:
        gate_id = gate["id"]
        proof = proofs[gate_id]
        require(isinstance(proof, dict), f"{gate_id}: gate evidence must be an object")
        require(required_gate_fields <= set(proof), f"{gate_id}: missing gate evidence fields")
        require(proof.get("gate_id") == gate_id, f"{gate_id}: gate ID mismatch")
        require(proof.get("result") == "pass", f"{gate_id}: gate did not pass")
        digest = proof.get("report_sha256")
        require(isinstance(digest, str) and re.fullmatch(r"[0-9a-fA-F]{64}", digest), f"{gate_id}: invalid report SHA-256")
        report_path = resolve_report(evidence_path, proof.get("report"))
        report_raw = read_bounded_regular(report_path, report_limit, f"{gate_id} report")
        actual = hashlib.sha256(report_raw).hexdigest()
        require(hmac.compare_digest(actual, digest.lower()), f"{gate_id}: report SHA-256 mismatch")
        report = load_json_bytes(report_raw, f"{gate_id} report")
        require(isinstance(report, dict), f"{gate_id}: report must be a JSON object")
        require(required_report_fields <= set(report), f"{gate_id}: report missing required fields")
        require(report.get("schema_version") == 1, f"{gate_id}: report schema mismatch")
        require(report.get("gate_id") == gate_id, f"{gate_id}: report gate ID mismatch")
        require(report.get("tested_commit") == current, f"{gate_id}: stale/wrong report commit")
        require(report.get("result") == "pass", f"{gate_id}: report result is not pass")
        performer = report.get("performed_by")
        reviewer = proof.get("reviewed_by")
        require(isinstance(performer, str) and performer.strip(), f"{gate_id}: performed_by required")
        require(isinstance(reviewer, str) and reviewer.strip(), f"{gate_id}: reviewed_by required")
        require(reviewer.strip().casefold() != performer.strip().casefold(), f"{gate_id}: independent reviewer required")

def main() -> None:
    data = json.loads((ROOT / "docs/migration/parity.json").read_text())
    gates = json.loads((ROOT / data["acceptance_gate_manifest"]).read_text())
    require(data["source_commit"] == "aa7f3720c42856f0e1d18742f78fba156227ee79", "wrong Python source commit")
    require(data["source_blob"] == "54bf6226eb80d1ef6c4f6087fb02579141ab134f", "wrong Python source blob")
    seen = set()
    for row in data["capabilities"]:
        require(row["id"] not in seen, f"duplicate capability {row['id']}")
        seen.add(row["id"])
        require(row["classification"] in {"mapped", "redesigned", "legacy-adapter", "intentionally-removed", "not-applicable"}, row["id"])
        require(row["status"] in {"implemented", "pending", "removed"}, row["id"])
        require(row["symbols"] and row["reason"] and row["verification_scope"], row["id"])
        require(len(row["source_lines"]) == 2 and row["source_lines"][0] <= row["source_lines"][1], row["id"])
        if row["status"] != "pending":
            require(row["evidence"], row["id"])
            for evidence in row["evidence"]:
                require((ROOT / evidence.split("#")[0]).is_file(), f"{row['id']}: missing {evidence}")
    require(len(seen) == 37, "Do not drop legacy responsibilities to manufacture coverage")
    parser = argparse.ArgumentParser()
    parser.add_argument("--require-complete", action="store_true")
    parser.add_argument("--evidence", type=Path)
    args = parser.parse_args()
    pending = [row["id"] for row in data["capabilities"] if row["status"] == "pending"]
    complete = False
    if args.require_complete:
        require(not pending, f"Unimplemented responsibilities: {pending}")
        require(args.evidence is not None, "Full replacement is NOT verified: external gate evidence is required")
        current = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
        validate_external_evidence(args.evidence, current, gates)
        complete = True
    print(json.dumps({
        "parity_ledger": "pass",
        "rows": len(seen),
        "implemented": sum(row["status"] == "implemented" for row in data["capabilities"]),
        "removed": sum(row["status"] == "removed" for row in data["capabilities"]),
        "pending_code": pending,
        "full_replacement_verified": complete,
        "separate_acceptance_gates": [gate["id"] for gate in gates["required_gates"]],
    }))

if __name__ == "__main__":
    main()
