#!/usr/bin/env python3
"""Disposable experiment; a successful probe is not production resume acceptance."""
import json
import os
from pathlib import Path
import plistlib
import subprocess
import tempfile


def command(*args):
    return subprocess.check_output(args, timeout=90)


root = Path(tempfile.mkdtemp(prefix="dandelion-mount-probe-"))
image = root / "source.dmg"
mount = root / "mounted"
mount.mkdir()
helper = root / "identity"
source = root / "identity.c"
report = {"kind": "macos-mount-identity-experiment", "samples": []}
attached = False
try:
    source.write_text('''#include <sys/mount.h>
#include <stdio.h>
int main(int argc, char **argv) {
  if (argc != 2) return 2;
  struct statfs info;
  if (statfs(argv[1], &info)) return 3;
  printf("{\\"fsid\\":[%d,%d],\\"filesystem\\":\\"%s\\"}\\n",
    info.f_fsid.val[0], info.f_fsid.val[1], info.f_fstypename);
  return 0;
}
''')
    command("/usr/bin/xcrun", "clang", "-Wall", "-Wextra", "-Werror", str(source), "-o", str(helper))
    report["bootSession"] = command("/usr/sbin/sysctl", "-n", "kern.bootsessionuuid").decode().strip()
    report["hostIdentity"] = json.loads(command(str(helper), str(root)))
    command("/usr/bin/hdiutil", "create", "-size", "128m", "-fs", "APFS", "-volname", "DandelionResumeProbe", str(image))
    for attempt in range(3):
        command("/usr/bin/hdiutil", "attach", "-nobrowse", "-mountpoint", str(mount), str(image))
        attached = True
        file = mount / "unchanged.txt"
        if attempt == 0:
            file.write_bytes(b"unchanged across independent mounts\n")
            os.sync()
        stat = file.stat()
        info = plistlib.loads(command("/usr/sbin/diskutil", "info", "-plist", str(mount)))
        snapshot = {"attempt": attempt, "volumeUUID": info.get("VolumeUUID"),
                    "device": info.get("DeviceIdentifier"),
                    "stamp": [stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns],
                    **json.loads(command(str(helper), str(file)))}
        report["samples"].append(snapshot)
        second = json.loads(command(str(helper), str(file)))
        assert second["fsid"] == snapshot["fsid"], "identity changed without remount"
        command("/usr/bin/hdiutil", "detach", str(mount))
        attached = False
    samples = report["samples"]
    assert len({s["volumeUUID"] for s in samples}) == 1 and samples[0]["volumeUUID"], "volume control changed"
    assert len({tuple(s["stamp"][1:]) for s in samples}) == 1, "unchanged file stamp control changed"
    report["fsidChangesOnEveryRemount"] = len({tuple(s["fsid"]) for s in samples}) == len(samples)
    report["productionResumeApproved"] = False
finally:
    if attached:
        command("/usr/bin/hdiutil", "detach", str(mount))
    destination = Path(os.environ["PROBE_REPORT"])
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))
