#!/usr/bin/env python3
"""Validate code coverage separately from externally reviewed full acceptance."""
import argparse
import json
from pathlib import Path
import subprocess
root = Path(__file__).resolve().parents[1]
data = json.loads((root / 'docs/migration/parity.json').read_text())
gates = json.loads((root / data['acceptance_gate_manifest']).read_text())
assert data['source_commit'] == 'aa7f3720c42856f0e1d18742f78fba156227ee79'
assert data['source_blob'] == '54bf6226eb80d1ef6c4f6087fb02579141ab134f'
seen = set()
for row in data['capabilities']:
    assert row['id'] not in seen
    seen.add(row['id'])
    assert row['classification'] in {'mapped', 'redesigned', 'legacy-adapter', 'intentionally-removed', 'not-applicable'}
    assert row['status'] in {'implemented', 'pending', 'removed'}
    assert row['symbols'] and row['reason'] and row['verification_scope']
    assert len(row['source_lines']) == 2 and row['source_lines'][0] <= row['source_lines'][1]
    if row['status'] != 'pending':
        assert row['evidence'], row['id']
        for evidence in row['evidence']:
            assert (root / evidence.split('#')[0]).is_file(), (row['id'], evidence)
assert len(seen) == 37, 'Do not drop legacy responsibilities to manufacture coverage'
parser = argparse.ArgumentParser()
parser.add_argument('--require-complete', action='store_true')
parser.add_argument('--evidence', type=Path)
args = parser.parse_args()
pending = [r['id'] for r in data['capabilities'] if r['status'] == 'pending']
complete = False
if args.require_complete:
    assert not pending, f'Unimplemented responsibilities: {pending}'
    assert args.evidence, 'Full replacement is NOT verified: physical hardware, production trust and controlled cutover evidence are required'
    assert args.evidence.stat().st_size <= 1024 * 1024
    evidence = json.loads(args.evidence.read_text())
    current = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
    assert evidence['tested_commit'] == current, 'Evidence must match the current checked-out revision'
    for gate in gates['required_gates']:
        proof = evidence.get('gates', {}).get(gate['id'], {})
        assert proof.get('result') == 'pass' and proof.get('artifact') and proof.get('reviewed_by'), gate['id']
    complete = True
print(json.dumps({'parity_ledger': 'pass', 'rows': len(seen),
                  'implemented': sum(r['status'] == 'implemented' for r in data['capabilities']),
                  'removed': sum(r['status'] == 'removed' for r in data['capabilities']),
                  'pending_code': pending, 'full_replacement_verified': complete,
                  'separate_acceptance_gates': [g['id'] for g in gates['required_gates']]}))
