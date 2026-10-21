"""Dry-run mode fences: no socket, workload data or real credentials required."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument('--binary', required=True)
args = parser.parse_args()
with tempfile.TemporaryDirectory() as directory:
    config = Path(directory) / 'modes.toml'
    config.write_text('''[transport]
mode = "tls"
[transport.tls]
ca_file = "/fixture/ca"
certificate_file = "/fixture/cert"
private_key_file = "/fixture/key"
wan_enabled = true
wan_allowlist = ["8.8.8.8:2443"]
[[transport.tls.peers]]
address = "127.0.0.1:2443"
server_name = "localhost"
class = "lan"
authorized = true
[[transport.tls.peers]]
address = "8.8.8.8:2443"
server_name = "authorized.example"
class = "wan"
authorized = true
''')
    def call(*flags):
        return subprocess.run([args.binary, '--config', str(config), 'run', '--dry-run', '--json', *flags], capture_output=True, text=True, timeout=10)
    loop = call('--local-loopback')
    assert loop.returncode == 0, loop.stderr
    assert json.loads(loop.stdout)['transport']['mode'] == 'loopback'
    lan = call('--lan-only')
    assert lan.returncode == 0, lan.stderr
    tls = json.loads(lan.stdout)['transport']['tls']
    assert [p['address'] for p in tls['peers']] == ['127.0.0.1:2443']
    assert tls['wan_enabled'] is False and tls['wan_allowlist'] == [] and tls['target_pool'] == []
    assert call('--local-loopback', '--lan-only').returncode != 0
    config.write_text(config.read_text().replace('class = "lan"', 'class = "private"'))
    assert call('--lan-only').returncode != 0
    source = Path(directory) / 'source'
    source.mkdir()
    (source / 'message').write_text('Rust stream')
    config.write_text('[state]\nenabled=false\n')
    streamed = subprocess.run([args.binary, '--config', str(config), 'run', '--source', str(source), '--repeat', '4', '--interval-ms', '500', '--progress-json', '--json'], capture_output=True, text=True, timeout=15)
    assert streamed.returncode == 0, streamed.stderr
    frames = [json.loads(line) for line in streamed.stdout.splitlines()]
    assert any(frame.get('event') == 'progress' and frame['pass_chunks_sent'] > 0 for frame in frames[:-1])
    assert frames[-1]['verified_payload_bytes'] == 4 * len(b'Rust stream')
print('mode fences: local overrides imported TLS, LAN excludes WAN, conflicting/empty local targets rejected')
