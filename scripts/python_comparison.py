#!/usr/bin/env python3
"""Equivalent offline packet work; never imports the original program's main."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import struct
import subprocess
import sys
import tempfile
import time
from disk_guard import check

p = argparse.ArgumentParser()
p.add_argument('--binary', type=Path)
p.add_argument('--rust-sha')
p.add_argument('--worker', action='store_true')
p.add_argument('--iterations', type=int, default=100000)
p.add_argument('--chunk-bytes', type=int, default=1024)
a = p.parse_args()
if a.worker:
    payload = bytes([0x5a]) * a.chunk_bytes
    start = time.perf_counter()
    for index in range(a.iterations):
        packet = struct.pack('!I', index) + hashlib.md5(payload).digest() + payload
    print(json.dumps({'packets': a.iterations, 'payload_bytes': a.chunk_bytes,
                      'elapsed_seconds': time.perf_counter() - start,
                      'final_packet_md5': hashlib.md5(packet).hexdigest(), 'network_bytes': 0}))
    sys.exit(0)
assert a.binary
if not a.rust_sha or len(a.rust_sha) != 40 or any(ch not in '0123456789abcdef' for ch in a.rust_sha.lower()):
    raise SystemExit('--rust-sha must be a full 40-hex commit SHA')
check(Path(tempfile.gettempdir()), 16 * 1024**2)
binary = str(a.binary.resolve())
env = {k: v for k, v in os.environ.items() if not k.startswith('DANDELION_')}
rows = []
with tempfile.TemporaryDirectory(prefix='dandelion-benchmark-') as tmp:
    for size in [1024, 1152]:
        for repeat in range(3):
            commands = {
                'python': [sys.executable, __file__, '--worker', '--iterations', str(a.iterations), '--chunk-bytes', str(size)],
                'rust': [binary, 'benchmark', 'legacy-packet', '--iterations', str(a.iterations), '--chunk-bytes', str(size), '--json'],
            }
            pair = {}
            for language in (['python', 'rust'] if repeat % 2 == 0 else ['rust', 'python']):
                metrics = Path(tmp) / f'{size}-{repeat}-{language}.json'
                proc = subprocess.run(['/usr/bin/time', '-f', '{"user_seconds":%U,"system_seconds":%S,"peak_rss_kib":%M}', '-o', str(metrics), *commands[language]],
                                      capture_output=True, text=True, env=env, timeout=30, check=True)
                result = json.loads(proc.stdout)
                result['process_metrics'] = json.loads(metrics.read_text())
                pair[language] = result
            assert pair['python']['final_packet_md5'] == pair['rust']['final_packet_md5']
            rows.append({'payload_bytes': size, 'repeat': repeat, **pair})
summary = []
for size in [1024, 1152]:
    samples = [r for r in rows if r['payload_bytes'] == size]
    py = statistics.median(r['python']['elapsed_seconds'] for r in samples)
    rs = statistics.median(r['rust']['elapsed_seconds'] for r in samples)
    summary.append({'payload_bytes': size, 'python_median_seconds': py, 'rust_median_seconds': rs,
                    'observed_packet_throughput_ratio_rust_over_python': py / rs})
print(json.dumps({
    'schema_version': 1, 'source_commit': 'aa7f3720c42856f0e1d18742f78fba156227ee79',
    'source_blob': '54bf6226eb80d1ef6c4f6087fb02579141ab134f', 'source_scope': 'reviewed pure packet expression only',
    'rust_sha': a.rust_sha, 'hardware': {'machine': platform.machine(), 'kernel': platform.release(), 'cpu_count': os.cpu_count()},
    'python': sys.version, 'workload': 'u32be + MD5 + 0x5a payload, allocating a complete packet each iteration; no network',
    'limitations': 'shared CI hardware; GNU time CPU resolution 0.01 s; process RSS includes startup; not whole-program cutover or TLS/drive throughput',
    'samples': rows, 'summary': summary,
}, indent=2))
