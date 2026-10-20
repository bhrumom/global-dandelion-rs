#!/usr/bin/env python3
# End-to-end offline release trust tests. Synthetic signing key, no installation.
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
from disk_guard import check
p=argparse.ArgumentParser();p.add_argument("--binary",type=Path,required=True);a=p.parse_args()
binary=str(a.binary.resolve());check(Path(tempfile.gettempdir()),32*1024**2)
env={k:v for k,v in os.environ.items() if not k.startswith("DANDELION_")}
def openssl(*args):return subprocess.run(["openssl",*args],capture_output=True,check=True,timeout=10)
with tempfile.TemporaryDirectory(prefix="dandelion-update-test-") as d:
    root=Path(d);key=root/"signing.key";pub=root/"key.raw";manifest=root/"manifest.json";sig=root/"manifest.sig";artifact=root/"artifact"
    openssl("genpkey","-algorithm","ED25519","-out",str(key));key.chmod(0o600)
    der=openssl("pkey","-in",str(key),"-pubout","-outform","DER").stdout
    assert der[:12].hex()=="302a300506032b6570032100" and len(der)==44
    pub.write_bytes(der[12:]);artifact.write_bytes(b"synthetic release artifact, never executed")
    doctor=subprocess.run([binary,"doctor","--json"],capture_output=True,text=True,env=env,check=True,timeout=10)
    target=json.loads(doctor.stdout)["target"]
    data={"schema_version":1,"version":"0.2.0","target":target,"size":artifact.stat().st_size,"sha256":hashlib.sha256(artifact.read_bytes()).hexdigest(),"expires_unix_seconds":int(time.time())+3600,"artifact_url":None}
    manifest.write_text(json.dumps(data,separators=(",",":")))
    openssl("pkeyutl","-sign","-rawin","-inkey",str(key),"-in",str(manifest),"-out",str(sig))
    trust=["--manifest",str(manifest),"--signature",str(sig),"--public-key",str(pub)]
    def cli(*args):return subprocess.run([binary,"update",*args],capture_output=True,text=True,env=env,timeout=10)
    valid=cli("check",*trust);assert valid.returncode==0,valid.stderr
    original=manifest.read_bytes();manifest.write_bytes(original+b" ");tampered=cli("check",*trust);assert tampered.returncode!=0;manifest.write_bytes(original)
    bad=root/"bad";bad.write_bytes(b"bad artifact")
    rejected=cli("stage",*trust,"--artifact",str(bad),"--directory",str(root));assert rejected.returncode!=0
    staged=cli("stage",*trust,"--artifact",str(artifact),"--directory",str(root));assert staged.returncode==0,staged.stderr
    report=json.loads(staged.stdout);assert report["activated"] is False
    assert (Path(report["directory"])/"artifact").read_bytes()==artifact.read_bytes()
    repeated=cli("stage",*trust,"--artifact",str(artifact),"--directory",str(root));assert repeated.returncode!=0
    print(json.dumps({"check":json.loads(valid.stdout),"staged":report,"tamper_exit":tampered.returncode,"corrupt_artifact_exit":rejected.returncode,"overwrite_exit":repeated.returncode},indent=2))
