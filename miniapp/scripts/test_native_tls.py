#!/usr/bin/env python3
"""Two package-owned workers and real mTLS; only authorized loopback fixtures."""
import json
import os
from pathlib import Path
import selectors
import signal
import socket
import subprocess
import sys
import tempfile
import time
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'scripts'))
from disk_guard import check

assert os.environ.get('GITHUB_ACTIONS') == 'true', 'Actions-only native execution'
runtime = Path(sys.argv[1]).resolve()
check(Path(tempfile.gettempdir()), 64 * 1024**2)


class Worker:
    def __init__(self, directory, config, source):
        directory.mkdir(mode=0o700)
        self.config, self.source = config, source
        self.process = subprocess.Popen(['node', str(runtime / 'worker.cjs')],
                                        env={**os.environ, 'FABUSHI_NATIVE_DATA_DIRECTORY': str(directory)},
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                        text=True, start_new_session=True)

    def send(self, value):
        self.process.stdin.write(json.dumps(value) + '\n')
        self.process.stdin.flush()

    def call(self, method, args=None):
        invocation = str(uuid.uuid4())
        self.send({'kind': 'invoke', 'id': invocation, 'method': method, 'args': args or {},
                   'scope': {'appId': 'org.fabushi.global-dandelion', 'version': '0.4.0',
                             'packageDigest': 'a' * 64, 'accountScope': 'b' * 64, 'accountGeneration': 1}})
        deadline = time.monotonic() + 30
        with selectors.DefaultSelector() as selector:
            selector.register(self.process.stdout, selectors.EVENT_READ)
            while time.monotonic() < deadline:
                assert selector.select(timeout=max(0.01, deadline-time.monotonic())), 'worker timeout'
                line = self.process.stdout.readline(1024 * 1024 + 1)
                assert line and len(line) <= 1024 * 1024, 'worker exited or oversized response'
                response = json.loads(line)
                assert response['id'] == invocation
                if response['kind'] == 'host':
                    if response['method'] == 'confirmation':
                        result = True
                    else:
                        assert response['method'] == 'file-picker'
                        path = self.source if response['mode'] == 'directory' else self.config
                        result = [{'path': str(path), 'name': path.name, 'size': 0}]
                    self.send({'kind': 'host-result', 'callId': response['callId'], 'result': result})
                else:
                    assert response['kind'] == 'result'
                    assert response['result']['ok'], response['result']
                    return response['result'].get('data')
        raise AssertionError('worker deadline')

    def close(self):
        try:
            os.killpg(self.process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            self.process.communicate(timeout=4)
        except subprocess.TimeoutExpired:
            os.killpg(self.process.pid, signal.SIGKILL)
            self.process.communicate(timeout=2)


def openssl(directory, *args):
    subprocess.run(['openssl', *args], cwd=directory, capture_output=True, check=True, timeout=10)


with tempfile.TemporaryDirectory(prefix='dandelion-worker-mtls-') as temporary:
    root = Path(temporary)
    openssl(root, 'req', '-x509', '-newkey', 'ec', '-pkeyopt', 'ec_paramgen_curve:P-256', '-nodes',
            '-keyout', 'ca.key', '-out', 'ca.pem', '-days', '1', '-subj', '/CN=Test Worker CA',
            '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign')
    for identity in ['server', 'client']:
        openssl(root, 'req', '-new', '-newkey', 'ec', '-pkeyopt', 'ec_paramgen_curve:P-256', '-nodes',
                '-keyout', identity + '.key', '-out', identity + '.csr', '-subj', '/CN=' + identity)
        extension = root / (identity + '.ext')
        extension.write_text('basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=serverAuth,clientAuth\nsubjectAltName=DNS:localhost,IP:127.0.0.1\n')
        openssl(root, 'x509', '-req', '-in', identity + '.csr', '-CA', 'ca.pem', '-CAkey', 'ca.key',
                '-CAcreateserial', '-out', identity + '.pem', '-days', '1', '-extfile', str(extension))
        (root / (identity + '.key')).chmod(0o600)
    (root / 'ca.key').chmod(0o600)
    source = root / 'source'
    source.mkdir()
    payload = bytes(range(251)) * 53
    (source / 'payload').write_bytes(payload)
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', 0))
        address = f'127.0.0.1:{listener.getsockname()[1]}'
    workers = []
    try:
        for identity in ['server', 'client']:
            config = root / (identity + '.toml')
            config.write_text(f'''[state]
enabled = true
directory = "/tmp/must-not-be-shared"
[transport]
mode = "tls"
[transport.tls]
ca_file = {json.dumps(str(root / 'ca.pem'))}
certificate_file = {json.dumps(str(root / (identity + '.pem')))}
private_key_file = {json.dumps(str(root / (identity + '.key')))}
timeout_ms = 3000
connect_attempts = 3
[[transport.tls.peers]]
address = "{address}"
server_name = "localhost"
class = "private"
authorized = true
''')
            worker = Worker(root / (identity + '-private'), config, source)
            workers.append(worker)
            assert worker.call('config.import')['imported']
        receiver, sender = workers
        assert receiver.call('transfer.configureReceiver', {'bind': address})['configured']
        operation = receiver.call('transfer.receive', {'targetPoolId': 'configured', 'once': True})['operationId']
        for attempt in range(30):
            state = receiver.call('transfer.status', {'operationId': operation})
            assert state['state'] == 'running', ('receiver failed before readiness', state)
            if state.get('ready'):
                assert state['ready'] == {'address': address, 'mutual_tls': True}
                break
            time.sleep(0.1)
        else:
            raise AssertionError(('native receiver never reported real readiness', state))
        selection = sender.call('selection.pick', {'mode': 'directory'})['selectionId']
        outgoing = sender.call('transfer.send', {'selectionId': selection, 'targetPoolId': 'configured'})['operationId']
        for worker, identifier in [(sender, outgoing), (receiver, operation)]:
            for attempt in range(30):
                status = worker.call('transfer.status', {'operationId': identifier})
                if status['state'] != 'running':
                    break
                time.sleep(0.1)
            assert status['state'] == 'complete', status
            assert status['result']['verified_payload_bytes'] == len(payload), status
            if worker is sender:
                assert status['result']['targets'][0]['authenticated']
        with socket.socket() as listener:
            listener.bind(('127.0.0.1', 0))
            cancel_address = f'127.0.0.1:{listener.getsockname()[1]}'
        receiver.call('transfer.configureReceiver', {'bind': cancel_address})
        cancellable = receiver.call('transfer.receive', {'targetPoolId': 'configured', 'once': False})['operationId']
        for attempt in range(30):
            if receiver.call('transfer.status', {'operationId': cancellable}).get('ready'):
                break
            time.sleep(0.1)
        else:
            raise AssertionError('cancellation fixture never became a listening receiver')
        receiver.call('transfer.cancel', {'operationId': cancellable})
        assert receiver.call('transfer.status', {'operationId': cancellable})['state'] == 'cancelled'
        assert (source / 'payload').read_bytes() == payload
        print(json.dumps({'packageOwnedWorkers': 2, 'mutualTls': True, 'verifiedPayloadBytes': len(payload),
                          'actualReadinessObserved': True, 'cancelVerified': True, 'originalFileUnchanged': True}))
    finally:
        for worker in workers:
            worker.close()
