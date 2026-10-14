#!/usr/bin/env python3
"""Real PTY acceptance isolated from the user's terminal."""
import argparse
import errno
import json
import os
from pathlib import Path
import pty
import select
import signal
import subprocess
import tempfile
import time
from disk_guard import check
p = argparse.ArgumentParser()
p.add_argument('--binary', type=Path, required=True)
a = p.parse_args()
check(Path(tempfile.gettempdir()), 16 * 1024**2)
with tempfile.TemporaryDirectory(prefix='dandelion-tui-') as tmp:
    (Path(tmp) / 'fixture').write_bytes(b'x' * 8192)
    master, slave = pty.openpty()
    env = {k: v for k, v in os.environ.items() if not k.startswith('DANDELION_')}
    env['TERM'] = 'xterm-256color'
    child = subprocess.Popen([str(a.binary.resolve()), 'run', '--source', tmp, '--tui', '--rate-bytes-per-second', '1', '--json'],
                             env=env, stdout=subprocess.PIPE, stderr=slave)
    os.close(slave)
    captured = bytearray()
    try:
        deadline = time.monotonic() + 5
        while b'\x1b[?1049h' not in captured and time.monotonic() < deadline:
            if select.select([master], [], [], 0.1)[0]:
                captured.extend(os.read(master, 65536))
            if child.poll() is not None:
                break
        assert b'\x1b[?1049h' in captured, 'dashboard did not enter alternate screen'
        child.send_signal(signal.SIGTERM)
        stdout, _ = child.communicate(timeout=4)
        while select.select([master], [], [], 0)[0]:
            try:
                data = os.read(master, 65536)
                if not data:
                    break
                captured.extend(data)
            except OSError as error:
                if error.errno == errno.EIO:
                    break
                raise
        assert child.returncode == 130, (child.returncode, captured.decode(errors='replace'))
        assert b'\x1b[?25h\x1b[?1049l' in captured, 'terminal restoration missing'
        result = json.loads(stdout)
        assert result['status'] == 'cancelled'
        print(json.dumps({'terminal_restored': True, 'signal_exit': child.returncode, 'snapshot': result}))
    finally:
        if child.poll() is None:
            child.kill()
            child.communicate(timeout=3)
        os.close(master)
