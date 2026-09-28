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
from urllib import error as urlerror
from urllib import request as urlrequest

ROOT = Path(__file__).resolve().parents[1]
HEX40 = re.compile(r"[0-9a-fA-F]{40}")
HEX64 = re.compile(r"[0-9a-fA-F]{64}")
GITHUB_REVIEW_API = re.compile(
    r"https://api\.github\.com/repos/([A-Za-z0-9_.-]+)/([A-Za-z0-9_.-]+)/pulls/([1-9][0-9]*)/reviews/([1-9][0-9]*)"
)
SOURCE_COMMIT = "aa7f3720c42856f0e1d18742f78fba156227ee79"
SOURCE_BLOB = "54bf6226eb80d1ef6c4f6087fb02579141ab134f"

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

def obj(value, label):
    require(isinstance(value, dict), f"{label}: must be an object")
    return value

def arr(value, label):
    require(isinstance(value, list), f"{label}: must be an array")
    return value

def text_field(value, key, label):
    value = obj(value, label).get(key)
    require(isinstance(value, str) and value.strip(), f"{label}.{key}: non-empty string required")
    return value.strip()

def bool_field(value, key, label, expected=True):
    actual = obj(value, label).get(key)
    require(actual is expected, f"{label}.{key}: must be {str(expected).lower()}")
    return actual

def positive_int_field(value, key, label):
    actual = obj(value, label).get(key)
    require(isinstance(actual, int) and not isinstance(actual, bool) and actual > 0,
            f"{label}.{key}: positive integer required")
    return actual

def exact_commit(value, key, label, current):
    actual = text_field(value, key, label)
    require(HEX40.fullmatch(actual) is not None, f"{label}.{key}: full 40-hex commit required")
    require(actual == current, f"{label}.{key}: must equal tested commit")
    return actual

def sha256_field(value, key, label):
    actual = text_field(value, key, label)
    require(HEX64.fullmatch(actual) is not None, f"{label}.{key}: SHA-256 required")
    return actual.lower()

def validate_current_ci(report, current, review_scope, current_ci_scope, github_fetcher=None):
    workflow = obj(report.get("workflow"), "current-ci.workflow")
    run_id = positive_int_field(workflow, "run_id", "current-ci.workflow")
    require(text_field(workflow, "conclusion", "current-ci.workflow") == "success",
            "current-ci.workflow.conclusion: must be success")
    exact_commit(workflow, "tested_commit", "current-ci.workflow", current)
    jobs = obj(report.get("jobs"), "current-ci.jobs")
    report_job_keys = ("quality_x86_64", "quality_aarch64", "dependency_policy", "required_acceptance_gate")
    for name in report_job_keys:
        require(jobs.get(name) == "success", f"current-ci.jobs.{name}: must be success")

    scope = obj(review_scope, "evidence_contract.review_scope")
    repository = text_field(scope, "repository", "evidence_contract.review_scope")
    pull_request = positive_int_field(scope, "pull_request", "evidence_contract.review_scope")
    ci_scope = obj(current_ci_scope, "evidence_contract.current_ci_scope")
    workflow_path = text_field(ci_scope, "workflow_path", "evidence_contract.current_ci_scope")
    api = github_fetcher or (lambda reference: fetch_github_json(reference, "current-ci GitHub API"))
    api_base = f"https://api.github.com/repos/{repository}"

    live_run = api(f"{api_base}/actions/runs/{run_id}")
    require(isinstance(live_run, dict), "current-ci: GitHub workflow run must be an object")
    require(live_run.get("id") == run_id, "current-ci: live workflow run ID mismatch")
    require(live_run.get("status") == "completed", "current-ci: live workflow run is not completed")
    require(live_run.get("conclusion") == "success", "current-ci: live workflow run did not succeed")
    require(live_run.get("head_sha") == current, "current-ci: live workflow run is not exact tested commit")
    require(live_run.get("event") == "pull_request", "current-ci: live workflow run must be a pull_request run")
    require(live_run.get("path") == workflow_path, "current-ci: live workflow path mismatch")
    live_repository = obj(live_run.get("repository"), "current-ci live workflow repository")
    require(text_field(live_repository, "full_name", "current-ci live workflow repository").casefold()
            == repository.casefold(), "current-ci: live workflow repository mismatch")
    pull_requests = arr(live_run.get("pull_requests"), "current-ci live workflow pull_requests")
    require(any(isinstance(item, dict) and item.get("number") == pull_request for item in pull_requests),
            "current-ci: live workflow run is not bound to configured pull request")

    live_jobs_response = api(f"{api_base}/actions/runs/{run_id}/jobs?per_page=100")
    live_jobs = arr(obj(live_jobs_response, "current-ci live jobs response").get("jobs"),
                    "current-ci live jobs")
    required_live_jobs = {
        "Quality / x86_64-unknown-linux-gnu": "quality_x86_64",
        "Quality / aarch64-unknown-linux-gnu": "quality_aarch64",
        "Dependency policy": "dependency_policy",
        "Required acceptance gate": "required_acceptance_gate",
    }
    for live_name, report_key in required_live_jobs.items():
        matches = [item for item in live_jobs if isinstance(item, dict) and item.get("name") == live_name]
        require(len(matches) == 1, f"current-ci: expected exactly one live job {live_name}")
        require(matches[0].get("status") == "completed" and matches[0].get("conclusion") == "success",
                f"current-ci: live job {live_name} did not succeed")
        require(jobs.get(report_key) == "success",
                f"current-ci.jobs.{report_key}: report/live job mismatch")

    artifacts = arr(report.get("artifacts"), "current-ci.artifacts")
    by_arch = {}
    for item in artifacts:
        item = obj(item, "current-ci artifact")
        arch = text_field(item, "architecture", "current-ci artifact")
        require(arch not in by_arch, f"current-ci: duplicate artifact architecture {arch}")
        by_arch[arch] = item
    expected = {"x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"}
    require(expected <= set(by_arch), "current-ci: both native architecture artifacts are required")

    live_artifacts_response = api(f"{api_base}/actions/runs/{run_id}/artifacts?per_page=100")
    live_artifacts = arr(obj(live_artifacts_response, "current-ci live artifacts response").get("artifacts"),
                         "current-ci live artifacts")
    for arch in sorted(expected):
        item = by_arch[arch]
        artifact_id = positive_int_field(item, "artifact_id", f"current-ci artifact {arch}")
        name = text_field(item, "artifact_name", f"current-ci artifact {arch}")
        require(current in name, f"current-ci artifact {arch}: artifact name must contain tested commit")
        github_zip_sha256 = sha256_field(item, "github_zip_sha256", f"current-ci artifact {arch}")
        sha256_field(item, "outer_archive_sha256", f"current-ci artifact {arch}")
        sha256_field(item, "binary_sha256", f"current-ci artifact {arch}")
        exact_commit(item, "workflow_head_commit", f"current-ci artifact {arch}", current)
        exact_commit(item, "build_info_commit", f"current-ci artifact {arch}", current)
        exact_commit(item, "comparison_commit", f"current-ci artifact {arch}", current)
        bool_field(item, "outer_archive_checksum_verified", f"current-ci artifact {arch}")
        bool_field(item, "inner_sha256sums_verified", f"current-ci artifact {arch}")
        bool_field(item, "self_test_passed", f"current-ci artifact {arch}")

        matches = [entry for entry in live_artifacts
                   if isinstance(entry, dict) and entry.get("id") == artifact_id]
        require(len(matches) == 1, f"current-ci artifact {arch}: live artifact ID not found uniquely")
        live = matches[0]
        require(live.get("name") == name, f"current-ci artifact {arch}: live artifact name mismatch")
        require(live.get("expired") is False, f"current-ci artifact {arch}: live artifact is expired")
        require(isinstance(live.get("size_in_bytes"), int) and live.get("size_in_bytes") > 0,
                f"current-ci artifact {arch}: live artifact is empty")
        require(str(live.get("digest", "")).lower() == f"sha256:{github_zip_sha256}".lower(),
                f"current-ci artifact {arch}: live GitHub ZIP digest mismatch")
        live_workflow = obj(live.get("workflow_run"), f"current-ci artifact {arch} live workflow")
        require(live_workflow.get("id") == run_id,
                f"current-ci artifact {arch}: live artifact belongs to different workflow run")
        require(live_workflow.get("head_sha") == current,
                f"current-ci artifact {arch}: live artifact workflow head mismatch")

def validate_physical_hotspot(report):
def validate_physical_hotspot(report):
    env = obj(report.get("environment"), "physical-hotspot.environment")
    text_field(env, "host", "physical-hotspot.environment")
    text_field(env, "kernel", "physical-hotspot.environment")
    hw = obj(report.get("hardware"), "physical-hotspot.hardware")
    text_field(hw, "interface", "physical-hotspot.hardware")
    text_field(hw, "driver", "physical-hotspot.hardware")
    text_field(hw, "device_identity", "physical-hotspot.hardware")
    bool_field(hw, "is_physical", "physical-hotspot.hardware")
    bool_field(hw, "virtual_or_mock", "physical-hotspot.hardware", False)
    bool_field(hw, "ap_supported", "physical-hotspot.hardware")
    life = obj(report.get("lifecycle"), "physical-hotspot.lifecycle")
    for key in ("plan_passed", "apply_passed", "active_ap_verified", "interruption_exercised",
                "rollback_verified", "owned_profile_removed"):
        bool_field(life, key, "physical-hotspot.lifecycle")
    prior = life.get("prior_connection_present")
    require(isinstance(prior, bool), "physical-hotspot.lifecycle.prior_connection_present: boolean required")
    if prior:
        bool_field(life, "prior_connection_restored", "physical-hotspot.lifecycle")

def validate_physical_hotplug(report):
    env = obj(report.get("environment"), "physical-hotplug.environment")
    text_field(env, "host", "physical-hotplug.environment")
    text_field(env, "kernel", "physical-hotplug.environment")
    hw = obj(report.get("hardware"), "physical-hotplug.hardware")
    original = obj(hw.get("original"), "physical-hotplug.hardware.original")
    replacement = obj(hw.get("replacement"), "physical-hotplug.hardware.replacement")
    for label, device in (("original", original), ("replacement", replacement)):
        text_field(device, "device_identity", f"physical-hotplug.hardware.{label}")
        text_field(device, "transport", f"physical-hotplug.hardware.{label}")
        bool_field(device, "is_physical", f"physical-hotplug.hardware.{label}")
        bool_field(device, "removable_or_hotplug", f"physical-hotplug.hardware.{label}")
    require(original["device_identity"] != replacement["device_identity"],
            "physical-hotplug: replacement device identity must differ from original")
    life = obj(report.get("lifecycle"), "physical-hotplug.lifecycle")
    for key in ("unmounted_during_active_scan", "original_reinserted",
                "replacement_mounted_at_same_path", "old_checkpoint_rejected_for_replacement",
                "resume_scope_changed_after_replacement", "service_recovered"):
        bool_field(life, key, "physical-hotplug.lifecycle")

def validate_controlled_cutover(report, current):
    architecture = text_field(report, "artifact_architecture", "controlled-cutover")
    require(architecture in {"x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"},
            "controlled-cutover.artifact_architecture: unsupported native artifact")
    positive_int_field(report, "artifact_id", "controlled-cutover")
    artifact_name = text_field(report, "artifact_name", "controlled-cutover")
    require(current in artifact_name and architecture in artifact_name,
            "controlled-cutover.artifact_name: must bind architecture and tested commit")
    sha256_field(report, "artifact_github_zip_sha256", "controlled-cutover")
    sha256_field(report, "artifact_outer_archive_sha256", "controlled-cutover")
    sha256_field(report, "artifact_binary_sha256", "controlled-cutover")
    exact_commit(report, "artifact_workflow_head_commit", "controlled-cutover", current)
    exact_commit(report, "artifact_build_info_commit", "controlled-cutover", current)
    exact_commit(report, "artifact_comparison_commit", "controlled-cutover", current)
    require(report.get("python_source_commit") == SOURCE_COMMIT,
            "controlled-cutover.python_source_commit: wrong pinned source")
    require(report.get("python_source_blob") == SOURCE_BLOB,
            "controlled-cutover.python_source_blob: wrong pinned source blob")
    repeats = positive_int_field(report, "repeats", "controlled-cutover")
    require(repeats >= 3, "controlled-cutover.repeats: at least 3 required")
    fixture = obj(report.get("fixture"), "controlled-cutover.fixture")
    positive_int_field(fixture, "regular_files", "controlled-cutover.fixture")
    positive_int_field(fixture, "total_file_bytes", "controlled-cutover.fixture")
    workloads = obj(report.get("workloads"), "controlled-cutover.workloads")
    for workload in ("metadata", "read", "transfer"):
        pair = obj(workloads.get(workload), f"controlled-cutover.workloads.{workload}")
        for language in ("python", "rust"):
            result = obj(pair.get(language), f"controlled-cutover.workloads.{workload}.{language}")
            raw = arr(result.get("raw"), f"controlled-cutover.workloads.{workload}.{language}.raw")
            require(len(raw) == repeats,
                    f"controlled-cutover.workloads.{workload}.{language}: raw sample count mismatch")
            summary = obj(result.get("summary"), f"controlled-cutover.workloads.{workload}.{language}.summary")
            require(summary.get("samples") == repeats,
                    f"controlled-cutover.workloads.{workload}.{language}.summary.samples mismatch")
    payload = fixture["total_file_bytes"]
    for row in workloads["read"]["python"]["raw"] + workloads["read"]["rust"]["raw"]:
        require(row.get("verified_payload_bytes") == payload,
                "controlled-cutover: read sample did not cover complete fixture")
    for row in workloads["transfer"]["python"]["raw"]:
        require(row.get("payload_bytes") == payload, "controlled-cutover: Python transfer source payload mismatch")
        require(row.get("receiver_payload_bytes") == payload,
                "controlled-cutover: Python transfer receiver did not verify complete payload")
        require(row.get("receiver_bad_md5") == 0, "controlled-cutover: Python transfer integrity failure")
        require(row.get("receiver_packets") == row.get("chunks_sent"),
                "controlled-cutover: Python transfer packet loss")
        require(row.get("receiver_wire_bytes") == row.get("attempted_wire_bytes"),
                "controlled-cutover: Python transfer wire-byte mismatch")
    for row in workloads["transfer"]["rust"]["raw"]:
        require(row.get("verified_payload_bytes") == payload,
                "controlled-cutover: Rust transfer did not verify complete payload")
        require(row.get("failed_sessions") == 0,
                "controlled-cutover: Rust transfer reported failed sessions")
    shutdown = obj(report.get("shutdown"), "controlled-cutover.shutdown")
    for language in ("python", "rust"):
        rows = arr(obj(shutdown.get(language), f"controlled-cutover.shutdown.{language}").get("raw"),
                   f"controlled-cutover.shutdown.{language}.raw")
        require(len(rows) == repeats, f"controlled-cutover.shutdown.{language}: raw sample count mismatch")

def validate_production_release_trust(report, current):
    env = obj(report.get("environment"), "production-release-trust.environment")
    text_field(env, "deployment", "production-release-trust.environment")
    signing = obj(report.get("signing"), "production-release-trust.signing")
    text_field(signing, "production_identity", "production-release-trust.signing")
    text_field(signing, "key_fingerprint", "production-release-trust.signing")
    bool_field(signing, "synthetic", "production-release-trust.signing", False)
    bool_field(signing, "signature_verified", "production-release-trust.signing")
    release = obj(report.get("release"), "production-release-trust.release")
    text_field(release, "origin", "production-release-trust.release")
    sha256_field(release, "artifact_sha256", "production-release-trust.release")
    exact_commit(release, "artifact_commit", "production-release-trust.release", current)
    bool_field(release, "fetched_from_production_origin", "production-release-trust.release")
    bool_field(release, "provenance_verified", "production-release-trust.release")
    activation = obj(report.get("activation"), "production-release-trust.activation")
    for key in ("staging_verified", "activation_performed", "health_verified",
                "rollback_exercised", "rollback_succeeded", "post_rollback_health_verified"):
        bool_field(activation, key, "production-release-trust.activation")

def validate_cross_gate_bindings(reports, current):
    current_ci = obj(reports.get("current-ci"), "cross-gate current-ci")
    cutover = obj(reports.get("controlled-cutover"), "cross-gate controlled-cutover")
    architecture = text_field(cutover, "artifact_architecture", "controlled-cutover")
    matches = [
        item for item in arr(current_ci.get("artifacts"), "current-ci.artifacts")
        if isinstance(item, dict) and item.get("architecture") == architecture
    ]
    require(len(matches) == 1,
            "controlled-cutover: exactly one matching current-ci artifact is required")
    artifact = matches[0]
    for cutover_key, ci_key in (
        ("artifact_id", "artifact_id"),
        ("artifact_name", "artifact_name"),
        ("artifact_workflow_head_commit", "workflow_head_commit"),
        ("artifact_build_info_commit", "build_info_commit"),
        ("artifact_comparison_commit", "comparison_commit"),
    ):
        require(cutover.get(cutover_key) == artifact.get(ci_key),
                f"controlled-cutover.{cutover_key}: must match current-ci artifact {ci_key}")
    for cutover_key, ci_key in (
        ("artifact_github_zip_sha256", "github_zip_sha256"),
        ("artifact_outer_archive_sha256", "outer_archive_sha256"),
        ("artifact_binary_sha256", "binary_sha256"),
    ):
        require(cutover.get(cutover_key, "").lower() == artifact.get(ci_key, "").lower(),
                f"controlled-cutover.{cutover_key}: must match current-ci artifact {ci_key}")
    exact_commit(cutover, "artifact_workflow_head_commit", "controlled-cutover", current)

def validate_gate_semantics(gate_id, report, current, review_scope, current_ci_scope, github_fetcher=None):
    if gate_id == "current-ci":
        validate_current_ci(report, current, review_scope, current_ci_scope, github_fetcher)
    elif gate_id == "physical-hotspot":
        validate_physical_hotspot(report)
    elif gate_id == "physical-hotplug":
        validate_physical_hotplug(report)
    elif gate_id == "controlled-cutover":
        validate_controlled_cutover(report, current)
    elif gate_id == "production-release-trust":
        validate_production_release_trust(report, current)
    else:
        raise RuntimeError(f"unknown acceptance gate {gate_id}")

def fetch_github_json(reference: str, label: str):
    require(reference.startswith("https://api.github.com/repos/"), f"{label}: canonical GitHub repository API URL required")
    request = urlrequest.Request(
        reference,
        headers={
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "global-dandelion-rs-acceptance-verifier",
        },
    )
    token = os.environ.get("GITHUB_TOKEN", "").strip()
    if token:
        request.add_header("Authorization", f"Bearer {token}")
    try:
        with urlrequest.urlopen(request, timeout=15) as response:
            require(getattr(response, "status", 200) == 200, f"{label}: GitHub lookup did not return 200")
            require(response.geturl() == reference, f"{label}: redirected GitHub API response rejected")
            raw = response.read(4_194_305)
    except (urlerror.HTTPError, urlerror.URLError, TimeoutError, OSError) as error:
        raise RuntimeError(f"{label}: cannot verify GitHub API resource: {error}") from error
    require(len(raw) <= 4_194_304, f"{label}: GitHub API response too large")
    return load_json_bytes(raw, label)


def fetch_github_review(reference: str) -> dict:
    match = GITHUB_REVIEW_API.fullmatch(reference)
    require(match is not None, "review_reference: GitHub pull-request review API URL required")
    review = fetch_github_json(reference, "GitHub review")
    require(isinstance(review, dict), "review_reference: GitHub review must be an object")
    require(review.get("id") == int(match.group(4)), "review_reference: GitHub review ID mismatch")
    return review


def validate_review_binding(def validate_review_binding(gate_id, proof, report, digest, current, review_scope, review_fetcher=None):
    performer = text_field(report, "performed_by", f"{gate_id} report")
    actor = text_field(report, "performed_actor", f"{gate_id} report")
    reviewer = text_field(proof, "reviewed_by", f"{gate_id} evidence")
    require(reviewer.casefold() != performer.casefold(), f"{gate_id}: reviewer must differ from performed_by")
    require(reviewer.casefold() != actor.casefold(), f"{gate_id}: reviewer must differ from performed_actor")
    require(proof.get("reviewed_report_sha256", "").lower() == digest.lower(),
            f"{gate_id}: reviewer binding must name exact report digest")
    require(proof.get("reviewed_commit") == current,
            f"{gate_id}: reviewer binding must name exact tested commit")
    require(proof.get("review_decision") == "approve",
            f"{gate_id}: reviewer decision must be approve")

    reference = text_field(proof, "review_reference", f"{gate_id} evidence")
    match = GITHUB_REVIEW_API.fullmatch(reference)
    require(match is not None, f"{gate_id}: review_reference must be an immutable GitHub PR review API URL")
    scope = obj(review_scope, "evidence_contract.review_scope")
    expected_repository = text_field(scope, "repository", "evidence_contract.review_scope")
    expected_pr = positive_int_field(scope, "pull_request", "evidence_contract.review_scope")
    referenced_repository = f"{match.group(1)}/{match.group(2)}"
    require(referenced_repository.casefold() == expected_repository.casefold(),
            f"{gate_id}: review_reference must target configured acceptance repository")
    require(int(match.group(3)) == expected_pr,
            f"{gate_id}: review_reference must target configured acceptance pull request")
    review = (review_fetcher or fetch_github_review)(reference)
    require(isinstance(review, dict), f"{gate_id}: review lookup must return an object")
    require(review.get("id") == int(match.group(4)), f"{gate_id}: external review ID mismatch")
    expected_pull_url = f"https://api.github.com/repos/{match.group(1)}/{match.group(2)}/pulls/{expected_pr}"
    require(review.get("pull_request_url") == expected_pull_url,
            f"{gate_id}: GitHub review pull_request_url does not match configured acceptance scope")
    user = obj(review.get("user"), f"{gate_id} GitHub review user")
    login = text_field(user, "login", f"{gate_id} GitHub review user")
    require(login.casefold() == reviewer.casefold(), f"{gate_id}: reviewed_by does not match GitHub review actor")
    require(str(review.get("state", "")).upper() == "APPROVED", f"{gate_id}: GitHub review is not APPROVED")
    require(review.get("commit_id") == current, f"{gate_id}: GitHub review is not anchored to exact tested commit")

    body = review.get("body")
    require(isinstance(body, str) and body.strip(), f"{gate_id}: GitHub review body attestation required")
    expected_body_sha = sha256_field(proof, "review_body_sha256", f"{gate_id} evidence")
    actual_body_sha = hashlib.sha256(body.encode("utf-8")).hexdigest()
    require(hmac.compare_digest(actual_body_sha, expected_body_sha),
            f"{gate_id}: GitHub review body SHA-256 mismatch")
    attestation = load_json_bytes(body.encode("utf-8"), f"{gate_id} GitHub review attestation")
    require(isinstance(attestation, dict), f"{gate_id}: GitHub review body must be a JSON attestation")
    require(attestation.get("schema_version") == 1, f"{gate_id}: review attestation schema mismatch")
    require(attestation.get("gate_id") == gate_id, f"{gate_id}: review attestation gate mismatch")
    require(attestation.get("tested_commit") == current, f"{gate_id}: review attestation commit mismatch")
    require(str(attestation.get("report_sha256", "")).lower() == digest.lower(),
            f"{gate_id}: review attestation report digest mismatch")
    require(attestation.get("decision") == "approve", f"{gate_id}: review attestation decision must be approve")


def validate_external_evidence(evidence_path: Path, current: str, gates: dict, review_fetcher=None, github_fetcher=None) -> None:
    contract = gates.get("evidence_contract", {})
    require(contract.get("hash_algorithm") == "sha256", "unsupported evidence hash contract")
    review_scope = obj(contract.get("review_scope"), "evidence_contract.review_scope")
    text_field(review_scope, "repository", "evidence_contract.review_scope")
    positive_int_field(review_scope, "pull_request", "evidence_contract.review_scope")
    current_ci_scope = obj(contract.get("current_ci_scope"), "evidence_contract.current_ci_scope")
    text_field(current_ci_scope, "workflow_path", "evidence_contract.current_ci_scope")
    manifest_limit = int(contract.get("max_manifest_bytes", 0))
    report_limit = int(contract.get("max_report_bytes", 0))
    raw = read_bounded_regular(evidence_path, manifest_limit, "evidence manifest")
    evidence = load_json_bytes(raw, "evidence manifest")
    require(isinstance(evidence, dict), "evidence manifest must be a JSON object")
    require(evidence.get("schema_version") == contract.get("schema_version"), "evidence schema mismatch")
    require(evidence.get("tested_commit") == current, "Evidence must match the current checked-out revision")
    proofs = evidence.get("gates")
    require(isinstance(proofs, dict), "evidence gates must be an object")
    required_ids = {gate["id"] for gate in gates["required_gates"]}
    require(set(proofs) == required_ids, "evidence must contain exactly the required gate IDs")
    required_gate_fields = set(contract.get("required_gate_fields", []))
    required_report_fields = set(contract.get("required_report_fields", []))
    reports = {}
    for gate in gates["required_gates"]:
        gate_id = gate["id"]
        proof = proofs[gate_id]
        require(isinstance(proof, dict), f"{gate_id}: gate evidence must be an object")
        require(required_gate_fields <= set(proof), f"{gate_id}: missing gate evidence fields")
        require(proof.get("gate_id") == gate_id, f"{gate_id}: gate ID mismatch")
        require(proof.get("result") == "pass", f"{gate_id}: gate did not pass")
        digest = proof.get("report_sha256")
        require(isinstance(digest, str) and HEX64.fullmatch(digest), f"{gate_id}: invalid report SHA-256")
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
        validate_review_binding(gate_id, proof, report, digest, current, review_scope, review_fetcher)
        validate_gate_semantics(gate_id, report, current, review_scope, current_ci_scope, github_fetcher)
        reports[gate_id] = report
    validate_cross_gate_bindings(reports, current)

def main() -> None:
    data = json.loads((ROOT / "docs/migration/parity.json").read_text())
    gates = json.loads((ROOT / data["acceptance_gate_manifest"]).read_text())
    require(data["source_commit"] == SOURCE_COMMIT, "wrong Python source commit")
    require(data["source_blob"] == SOURCE_BLOB, "wrong Python source blob")
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
