#!/usr/bin/env python3
# Ephemeral PKI and real native mTLS acceptance; loopback traffic only.
import argparse
import json
import os
from pathlib import Path
import selectors
import subprocess
import tempfile
from disk_guard import check

parser=argparse.ArgumentParser()
parser.add_argument("--binary",type=Path,required=True)
args=parser.parse_args()
binary=str(args.binary.resolve())
check(Path(tempfile.gettempdir()),32*1024**2)
env={k:v for k,v in os.environ.items() if not k.startswith("DANDELION_")}
results={}

def openssl(root,*args):
    subprocess.run(["openssl",*args],cwd=root,capture_output=True,check=True,timeout=10)

def ca(root,name):
    openssl(root,"req","-x509","-newkey","ec","-pkeyopt","ec_paramgen_curve:P-256","-nodes",
            "-keyout",f"{name}.key","-out",f"{name}.pem","-days","1","-subj",f"/CN=Test {name}",
            "-addext","basicConstraints=critical,CA:TRUE","-addext","keyUsage=critical,keyCertSign,cRLSign")
    (root/f"{name}.key").chmod(0o600)

def identity(root,name,issuer):
    openssl(root,"req","-new","-newkey","ec","-pkeyopt","ec_paramgen_curve:P-256","-nodes",
            "-keyout",f"{name}.key","-out",f"{name}.csr","-subj",f"/CN={name}")
    (root/f"{name}.key").chmod(0o600)
    ext=root/f"{name}.ext"
    ext.write_text("basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=serverAuth,clientAuth\nsubjectAltName=DNS:localhost,IP:127.0.0.1\n")
    openssl(root,"x509","-req","-in",f"{name}.csr","-CA",f"{issuer}.pem","-CAkey",f"{issuer}.key",
            "-CAcreateserial","-out",f"{name}.pem","-days","1","-extfile",str(ext))

def ready(proc):
    with selectors.DefaultSelector() as selector:
        selector.register(proc.stdout,selectors.EVENT_READ)
        assert selector.select(timeout=5),"receiver did not become ready"
    line=proc.stdout.readline()
    assert line,"receiver failed before readiness"
    event=json.loads(line)
    assert event["mutual_tls"] and event["event"]=="ready"
    return event["address"]

with tempfile.TemporaryDirectory(prefix="dandelion-tls-test-") as directory:
    root=Path(directory)
    ca(root,"ca");ca(root,"foreign")
    identity(root,"server","ca");identity(root,"client","ca");identity(root,"untrusted","foreign")
    source=root/"source";source.mkdir()
    (source/"fixture").write_bytes(bytes(range(251))*53)
    payload=(source/"fixture").read_bytes()
    for case in ["success","wrong-server-name","untrusted-client","budget"]:
        receiver=subprocess.Popen([binary,"receive","--bind","127.0.0.1:0","--once","--json",
            "--ca-file",str(root/"ca.pem"),"--certificate-file",str(root/"server.pem"),
            "--private-key-file",str(root/"server.key"),"--timeout-ms","3000"],env=env,
            stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
        try:
            address=ready(receiver)
            name="wrong.invalid" if case=="wrong-server-name" else "localhost"
            identity_name="untrusted" if case=="untrusted-client" else "client"
            config=root/f"{case}.toml"
            config.write_text(f"""[scanner]
roots = [{json.dumps(str(source))}]
[pacing]
traffic_limit_bytes = {1 if case == 'budget' else 1048576}
[transport]
mode = "tls"
[transport.tls]
ca_file = {json.dumps(str(root/'ca.pem'))}
certificate_file = {json.dumps(str(root/(identity_name+'.pem')))}
private_key_file = {json.dumps(str(root/(identity_name+'.key')))}
timeout_ms = 3000
[[transport.tls.peers]]
address = "{address}"
server_name = "{name}"
class = "private"
authorized = true
""")
            client=subprocess.run([binary,"--config",str(config),"run","--json"],env=env,capture_output=True,text=True,timeout=10)
            stdout,stderr=receiver.communicate(timeout=6)
            report=json.loads(stdout.strip())
            if case=="success":
                assert client.returncode==0,(client.returncode,client.stderr)
                stats=json.loads(client.stdout)
                assert stats["verified_payload_bytes"]==len(payload),stats
                assert stats["attempted_wire_bytes"]>len(payload),"TLS handshake/record bytes must be included"
                assert report["verified_payload_bytes"]==len(payload),report
                assert report["failed_sessions"]==0 and receiver.returncode==0,(report,stderr)
                results[case]={"sender":stats,"receiver":report}
            else:
                assert client.returncode!=0,(case,client.stdout)
                assert report["verified_payload_bytes"]==0,(case,report)
                assert receiver.returncode==4,(case,receiver.returncode,stderr)
                if case=="budget":assert client.returncode==75,client.stderr
                results[case]={"sender_exit":client.returncode,"receiver":report}
            assert (source/"fixture").read_bytes()==payload
        finally:
            if receiver.poll() is None:
                receiver.kill();receiver.communicate(timeout=3)
print(json.dumps(results,indent=2))
