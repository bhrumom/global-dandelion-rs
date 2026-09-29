#!/usr/bin/env python3
"""Exercise the supported systemd service lifecycle on an ephemeral GitHub Actions runner.

This script is intentionally operational: it installs only temporary CI units, uses
loopback transport, never enables public transmission, and removes all units on exit.
"""
import argparse
import getpass
import grp
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time

from disk_guard import check

parser = argparse.ArgumentParser()
parser.add_argument("--binary", type=Path, required=True)
parser.add_argument(
    "--template",
    type=Path,
    default=Path("packaging/systemd/dandelion.service"),
)
args = parser.parse_args()
binary = args.binary.resolve()
template = args.template.resolve()
if not binary.is_file() or not os.access(binary, os.X_OK):
    raise RuntimeError("systemd acceptance requires an executable dandelion binary")
if not template.is_file():
    raise RuntimeError("systemd service template missing")
for command in ("systemctl", "systemd-analyze", "journalctl", "sudo"):
    if shutil.which(command) is None:
        raise RuntimeError(f"required CI service tool missing: {command}")

check(Path(tempfile.gettempdir()), 64 * 1024**2)
raw = template.read_text()
required_template = [
    "Type=simple",
    "Restart=on-failure",
    "RestartSec=2s",
    "StartLimitIntervalSec=60",
    "StartLimitBurst=3",
    "RestartPreventExitStatus=2 4 75",
    "SuccessExitStatus=130",
    "TimeoutStopSec=15",
    "StandardOutput=journal",
    "StandardError=journal",
    "StateDirectory=dandelion",
    "WantedBy=multi-user.target",
    "NoNewPrivileges=true",
]
missing = [item for item in required_template if item not in raw]
if missing:
    raise RuntimeError(f"systemd template contract missing: {missing}")
subprocess.run(
    ["systemd-analyze", "verify", str(template)],
    check=True,
    text=True,
    capture_output=True,
    timeout=20,
)

results = {
    "template_verified": True,
    "boot_enablement": False,
    "background_active": False,
    "duplicate_instance_rejected": False,
    "graceful_sigterm": False,
    "durable_state_recovered": False,
    "journald_output": False,
    "bounded_restart_policy": False,
}
suffix = f"{os.environ.get('GITHUB_RUN_ID', 'local')}-{os.getpid()}"
service_name = f"dandelion-ci-{suffix}.service"
failure_name = f"dandelion-ci-failure-{suffix}.service"
system_service = Path("/etc/systemd/system") / service_name
system_failure = Path("/etc/systemd/system") / failure_name
user = getpass.getuser()
group = grp.getgrgid(os.getgid()).gr_name


def run(*argv, check_result=True, timeout=20):
    return subprocess.run(
        list(argv),
        check=check_result,
        text=True,
        capture_output=True,
        timeout=timeout,
    )


def sudo(*argv, check_result=True, timeout=20):
    return run("sudo", *argv, check_result=check_result, timeout=timeout)


def wait_active(name, deadline=6.0):
    stop = time.monotonic() + deadline
    while time.monotonic() < stop:
        if sudo("systemctl", "is-active", "--quiet", name, check_result=False).returncode == 0:
            return True
        time.sleep(0.1)
    return False


def toml_string(value: Path) -> str:
    return json.dumps(str(value))


try:
    with tempfile.TemporaryDirectory(prefix="dandelion-systemd-") as directory:
        root = Path(directory)
        source = root / "source"
        state = root / "state"
        source.mkdir()
        (source / "payload.bin").write_bytes(b"systemd-acceptance" * 1024)
        config = root / "service.toml"
        config.write_text(
            f"""[runtime]
queue_capacity = 4
shutdown_timeout_ms = 3000

[scanner]
roots = [{toml_string(source)}]
allowed_mount_prefixes = []

[pacing]
bytes_per_second = 1048576
traffic_limit_bytes = 67108864

[state]
enabled = true
directory = {toml_string(state)}
lifetime_limit_bytes = 1073741824

[transport]
mode = "loopback"
"""
        )

        unit = root / service_name
        unit.write_text(
            f"""[Unit]
Description=Global Dandelion CI lifecycle acceptance
StartLimitIntervalSec=5
StartLimitBurst=3

[Service]
Type=simple
User={user}
Group={group}
ExecStart={binary} --config {config} --log-format json run --watch --interval-ms 200 --json
Restart=on-failure
RestartSec=1s
RestartPreventExitStatus=2 4 75
SuccessExitStatus=130
TimeoutStopSec=5
StandardOutput=journal
StandardError=journal
NoNewPrivileges=true

[Install]
WantedBy=multi-user.target
"""
        )
        sudo("cp", str(unit), str(system_service))
        sudo("systemctl", "daemon-reload")
        sudo("systemctl", "enable", "--runtime", service_name)
        enabled = sudo(
            "systemctl", "is-enabled", service_name, check_result=False
        ).stdout.strip()
        if enabled not in {"enabled", "enabled-runtime"}:
            raise RuntimeError(f"temporary service was not enable-able at boot: {enabled}")
        results["boot_enablement"] = True

        sudo("systemctl", "start", service_name)
        if not wait_active(service_name):
            status = sudo(
                "systemctl", "status", service_name, "--no-pager", check_result=False
            )
            raise RuntimeError(f"service did not remain active: {status.stdout}\n{status.stderr}")
        results["background_active"] = True

        duplicate = run(
            str(binary),
            "--config",
            str(config),
            "run",
            "--watch",
            "--interval-ms",
            "200",
            "--json",
            check_result=False,
            timeout=5,
        )
        if duplicate.returncode != 1 or "another process owns the state directory" not in duplicate.stderr:
            raise RuntimeError(
                f"duplicate instance did not fail closed: rc={duplicate.returncode} stderr={duplicate.stderr}"
            )
        results["duplicate_instance_rejected"] = True

        started = time.monotonic()
        sudo("systemctl", "stop", service_name)
        elapsed = time.monotonic() - started
        if elapsed >= 5.0:
            raise RuntimeError(f"SIGTERM shutdown exceeded test deadline: {elapsed}")
        if sudo(
            "systemctl", "is-active", "--quiet", service_name, check_result=False
        ).returncode == 0:
            raise RuntimeError("service remained active after stop")
        results["graceful_sigterm"] = True
        results["shutdown_seconds"] = elapsed

        state_file = state / "state.json"
        if not state_file.is_file() or not (state / "writer.lock").is_file():
            raise RuntimeError("durable state was not created before service stop")
        first_state = json.loads(state_file.read_text())
        first_revision = int(first_state["revision"])

        sudo("systemctl", "start", service_name)
        if not wait_active(service_name):
            raise RuntimeError("service did not recover using existing durable state")
        time.sleep(0.3)
        sudo("systemctl", "stop", service_name)
        second_state = json.loads(state_file.read_text())
        if int(second_state["revision"]) <= first_revision:
            raise RuntimeError("service restart did not advance durable state revision")
        results["durable_state_recovered"] = True
        results["state_revision_before_restart"] = first_revision
        results["state_revision_after_restart"] = int(second_state["revision"])

        journal = sudo(
            "journalctl",
            "-u",
            service_name,
            "--no-pager",
            "-n",
            "100",
            "-o",
            "cat",
            check_result=False,
        )
        if journal.returncode != 0 or not journal.stdout.strip():
            raise RuntimeError("service produced no journal-readable output")
        if '"status"' not in journal.stdout:
            raise RuntimeError("journal output did not contain structured/final runtime status")
        results["journald_output"] = True

        failure = root / failure_name
        failure.write_text(
            """[Unit]
Description=Global Dandelion bounded restart policy acceptance
StartLimitIntervalSec=5
StartLimitBurst=3

[Service]
Type=simple
ExecStart=/usr/bin/false
Restart=on-failure
RestartSec=200ms
"""
        )
        sudo("cp", str(failure), str(system_failure))
        sudo("systemctl", "daemon-reload")
        sudo("systemctl", "start", failure_name, check_result=False)
        time.sleep(1.5)
        show = sudo(
            "systemctl",
            "show",
            failure_name,
            "--property=NRestarts",
            "--property=Result",
            "--property=ActiveState",
            check_result=False,
        )
        fields = dict(
            line.split("=", 1)
            for line in show.stdout.splitlines()
            if "=" in line
        )
        restarts = int(fields.get("NRestarts", "0"))
        if not (1 <= restarts <= 3):
            raise RuntimeError(f"restart policy was not bounded as expected: {fields}")
        if fields.get("ActiveState") == "active":
            raise RuntimeError(f"failure service unexpectedly remained active: {fields}")
        results["bounded_restart_policy"] = True
        results["failure_restarts"] = restarts

finally:
    for name in (service_name, failure_name):
        sudo("systemctl", "stop", name, check_result=False, timeout=10)
        sudo("systemctl", "disable", "--runtime", name, check_result=False, timeout=10)
    for path in (system_service, system_failure):
        sudo("rm", "-f", str(path), check_result=False, timeout=10)
    sudo("systemctl", "daemon-reload", check_result=False, timeout=10)

print(json.dumps(results, indent=2, sort_keys=True))
