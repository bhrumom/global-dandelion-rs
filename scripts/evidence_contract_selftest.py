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

def report_for(gate_id: str) -> dict:
    base = {
        "schema_version": 1,
        "gate_id": gate_id,
        "tested_commit": current,
        "result": "pass",
        "performed_by": f"synthetic-executor:{gate_id}",
    }
    if gate_id == "current-ci":
        base.update({
            "workflow": {"run_id": 1, "conclusion": "success", "tested_commit": current},
            "jobs": {
                "quality_x86_64": "success",
                "quality_aarch64": "success",
                "dependency_policy": "success",
                "required_acceptance_gate": "success",
            },
            "artifacts": [
                {
                    "architecture": arch,
                    "artifact_id": index + 1,
                    "artifact_name": f"dandelion-{arch}-{current}",
                    "github_zip_sha256": "a" * 64,
                    "outer_archive_sha256": "c" * 64,
                    "binary_sha256": "d" * 64,
                    "workflow_head_commit": current,
                    "build_info_commit": current,
                    "comparison_commit": current,
                    "outer_archive_checksum_verified": True,
                    "inner_sha256sums_verified": True,
                    "self_test_passed": True,
                }
                for index, arch in enumerate(("x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"))
            ],
        })
    elif gate_id == "physical-hotspot":
        base.update({
            "environment": {"host": "synthetic-host", "kernel": "synthetic-kernel"},
            "hardware": {
                "interface": "wlan0", "driver": "synthetic-driver", "device_identity": "synthetic-radio",
                "is_physical": True, "virtual_or_mock": False, "ap_supported": True,
            },
            "lifecycle": {
                "plan_passed": True, "apply_passed": True, "active_ap_verified": True,
                "interruption_exercised": True, "rollback_verified": True,
                "owned_profile_removed": True, "prior_connection_present": True,
                "prior_connection_restored": True,
            },
        })
    elif gate_id == "physical-hotplug":
        base.update({
            "environment": {"host": "synthetic-host", "kernel": "synthetic-kernel"},
            "hardware": {
                "original": {"device_identity": "disk-A", "transport": "usb", "is_physical": True, "removable_or_hotplug": True},
                "replacement": {"device_identity": "disk-B", "transport": "usb", "is_physical": True, "removable_or_hotplug": True},
            },
            "lifecycle": {
                "unmounted_during_active_scan": True, "original_reinserted": True,
                "replacement_mounted_at_same_path": True, "old_checkpoint_rejected_for_replacement": True,
                "resume_scope_changed_after_replacement": True, "service_recovered": True,
            },
        })
    elif gate_id == "controlled-cutover":
        payload = 1024
        def sample(language, workload):
            common = {"process_metrics": {"wall_seconds": 1.0, "user_seconds": 0.1, "system_seconds": 0.1, "peak_rss_kib": 1024}}
            if workload == "metadata":
                common.update({"regular_files": 1})
            elif workload == "read":
                common.update({"verified_payload_bytes": payload})
            elif language == "python":
                common.update({
                    "payload_bytes": payload, "receiver_payload_bytes": payload,
                    "receiver_bad_md5": 0, "receiver_packets": 1, "chunks_sent": 1,
                    "receiver_wire_bytes": payload + 20, "attempted_wire_bytes": payload + 20,
                })
            else:
                common.update({"verified_payload_bytes": payload, "failed_sessions": 0})
            return common
        repeats = 3
        workloads = {}
        for workload in ("metadata", "read", "transfer"):
            workloads[workload] = {}
            for language in ("python", "rust"):
                workloads[workload][language] = {
                    "raw": [sample(language, workload) for _ in range(repeats)],
                    "summary": {"samples": repeats},
                }
        base.update({
            "artifact_architecture": "x86_64-unknown-linux-gnu",
            "artifact_id": 1,
            "artifact_name": f"dandelion-x86_64-unknown-linux-gnu-{current}",
            "artifact_github_zip_sha256": "a" * 64,
            "artifact_outer_archive_sha256": "c" * 64,
            "artifact_binary_sha256": "d" * 64,
            "artifact_workflow_head_commit": current,
            "artifact_build_info_commit": current,
            "artifact_comparison_commit": current,
            "python_source_commit": "aa7f3720c42856f0e1d18742f78fba156227ee79",
            "python_source_blob": "54bf6226eb80d1ef6c4f6087fb02579141ab134f",
            "fixture": {"regular_files": 1, "total_file_bytes": payload},
            "repeats": repeats,
            "hardware": {"machine": "synthetic", "kernel": "synthetic", "cpu_count": 1},
            "workloads": workloads,
            "shutdown": {
                "python": {"raw": [{"shutdown_seconds": 0.1} for _ in range(repeats)]},
                "rust": {"raw": [{"shutdown_seconds": 0.1} for _ in range(repeats)]},
            },
        })
    elif gate_id == "production-release-trust":
        base.update({
            "environment": {"deployment": "synthetic-production-shaped-fixture"},
            "signing": {
                "production_identity": "synthetic-id-for-checker-only",
                "key_fingerprint": "synthetic-fingerprint", "synthetic": False, "signature_verified": True,
            },
            "release": {
                "origin": "https://example.invalid/release", "artifact_sha256": "b" * 64,
                "artifact_commit": current, "fetched_from_production_origin": True, "provenance_verified": True,
            },
            "activation": {
                "staging_verified": True, "activation_performed": True, "health_verified": True,
                "rollback_exercised": True, "rollback_succeeded": True, "post_rollback_health_verified": True,
            },
        })
    return base

with tempfile.TemporaryDirectory(prefix="dandelion-evidence-contract-") as directory:
    root = Path(directory)
    proof = {"schema_version": gates["evidence_contract"]["schema_version"], "tested_commit": current, "gates": {}}
    report_paths = {}
    for gate in gates["required_gates"]:
        gate_id = gate["id"]
        report = report_for(gate_id)
        path = root / f"{gate_id}.json"
        raw = (json.dumps(report, sort_keys=True) + "\n").encode()
        path.write_bytes(raw)
        digest = hashlib.sha256(raw).hexdigest()
        report_paths[gate_id] = path
        proof["gates"][gate_id] = {
            "gate_id": gate_id,
            "result": "pass",
            "report": path.name,
            "report_sha256": digest,
            "reviewed_by": f"synthetic-reviewer:{gate_id}",
            "reviewed_report_sha256": digest,
            "reviewed_commit": current,
            "review_decision": "approve",
            "review_reference": f"synthetic://review/{gate_id}/{digest}",
        }
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

    wrong_review_digest = copy.deepcopy(proof)
    wrong_review_digest["gates"][first]["reviewed_report_sha256"] = "0" * 64
    manifest.write_text(json.dumps(wrong_review_digest, indent=2) + "\n")
    expect_failure(manifest, "reviewer did not bind exact report digest")

    hotspot = "physical-hotspot"
    strong_hotspot = report_paths[hotspot].read_bytes()
    weak = json.loads(strong_hotspot)
    del weak["hardware"]
    weak_raw = (json.dumps(weak, sort_keys=True) + "\n").encode()
    report_paths[hotspot].write_bytes(weak_raw)
    weak_proof = copy.deepcopy(proof)
    weak_digest = hashlib.sha256(weak_raw).hexdigest()
    weak_proof["gates"][hotspot]["report_sha256"] = weak_digest
    weak_proof["gates"][hotspot]["reviewed_report_sha256"] = weak_digest
    manifest.write_text(json.dumps(weak_proof, indent=2) + "\n")
    expect_failure(manifest, "generic pass report bypassed gate-specific semantics")
    report_paths[hotspot].write_bytes(strong_hotspot)

    cutover = "controlled-cutover"
    mismatched = json.loads(report_paths[cutover].read_text())
    mismatched["artifact_binary_sha256"] = "e" * 64
    mismatched_raw = (json.dumps(mismatched, sort_keys=True) + "\n").encode()
    report_paths[cutover].write_bytes(mismatched_raw)
    mismatch_proof = copy.deepcopy(proof)
    mismatch_digest = hashlib.sha256(mismatched_raw).hexdigest()
    mismatch_proof["gates"][cutover]["report_sha256"] = mismatch_digest
    mismatch_proof["gates"][cutover]["reviewed_report_sha256"] = mismatch_digest
    manifest.write_text(json.dumps(mismatch_proof, indent=2) + "\n")
    expect_failure(manifest, "controlled cutover used a different binary than current-ci")

print(json.dumps({
    "evidence_contract": "pass",
    "synthetic_only": True,
    "gate_specific_semantics": True,
    "cross_gate_artifact_binding": True,
}))
