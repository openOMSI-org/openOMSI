#!/usr/bin/env python3
"""Exercise the real CLI and capture parser. Never loads an NR runtime.

The only runtime test uses an empty directory and must fail before DLL loading.
Optional Wine tests the Windows executable, not native Windows/GPU support.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', type=Path, required=True)
    parser.add_argument('--capture', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True, help='new evidence directory')
    parser.add_argument('--wine', action='store_true')
    parser.add_argument('--cpu-fixture', action='store_true', help='create explicitly synthetic CPU protocol inputs for GPU-less CI; never neural output')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    args.output = args.output.resolve()
    results = []
    if args.cpu_fixture:
        args.capture.mkdir(parents=True, exist_ok=False)
        for number in (1, 2):
            header = bytearray(384)
            struct.pack_into('<8sIIQQIIIIQIi', header, 0, b'OMSINR01', 1, 1, 1, number, 1, 2, 2, 7 if number == 1 else 5, 68, 0, 0)
            camera = [0.0] * 80
            for matrix in range(4):
                for diagonal in range(4):
                    camera[matrix * 16 + diagonal * 5] = 1.0
            camera[76:] = [0.1, 10.0, 1.0, 1.0]
            struct.pack_into('<80f', header, 64, *camera)
            planes = bytes([20, 30, 40, 255] * 4) + bytes(32) + struct.pack('<4f', *([0.5] * 4)) + bytes([255] * 4)
            (args.capture / f'{number:08}.omsi-nr').write_bytes(header + planes)
        (args.capture / 'capture.json').write_text('{"source":"synthetic CPU protocol fixture; no GPU rendering or NVIDIA evaluation"}\n')

    def native(path):
        path = str(Path(path).resolve())
        return 'Z:' + path.replace('/', '\\') if args.wine else path

    def run(name, arguments, success):
        command = (['wine', str(args.host.resolve())] if args.wine else [str(args.host.resolve())]) + arguments
        completed = subprocess.run(command, capture_output=True, text=True, timeout=60, env=os.environ.copy())
        (args.output / (name + '.log')).write_text(completed.stdout + completed.stderr, encoding='utf-8')
        assert (completed.returncode == 0) == success, (name, completed.returncode, completed.stderr)
        results.append({'test': name, 'command': command, 'exit_code': completed.returncode, 'passed': True})

    run('self-test', ['--self-test'], True)
    run('build-info', ['--build-info'], True)
    run('inspect-real-sequence', ['--inspect-frame', native(args.capture)], True)
    captures = sorted(args.capture.glob('*.omsi-nr'))
    assert len(captures) >= 2, 'a temporal pair is required'
    packet = bytearray(captures[1].read_bytes())
    width, height = struct.unpack_from('<II', packet, 36)
    n = width * height
    assert len(packet) == 384 + n * 17
    inputs = []
    for file in captures:
        data = file.read_bytes()
        inputs.append({'file': file.name, 'sha256': hashlib.sha256(data).hexdigest()})
    for name, mutate in [
        ('truncated', lambda data: data[:-1]),
        ('depth-out-of-range', lambda data: data[:384+n*12] + struct.pack('<f', 2.0) + data[388+n*12:]),
        ('motion-nan', lambda data: data[:384+n*4] + struct.pack('<f', float('nan')) + data[388+n*4:]),
        ('unknown-color-space', lambda data: data[:44] + struct.pack('<I', 1) + data[48:]),
    ]:
        file = args.output / (name + '.omsi-nr')
        file.write_bytes(mutate(packet))
        run(name, ['--inspect-frame', native(file)], False)
    run('unknown-option', ['--self-test', '--invented'], False)
    if os.name == 'nt' or args.wine:
        missing = args.output / 'empty-runtime'
        missing.mkdir()
        run('diagnose-missing', ['--diagnose', '--runtime', native(missing)], True)
        report = args.output / 'missing-runtime-report'
        run('reject-missing-runtime', ['--replay-frame', native(args.capture), '--runtime', native(missing),
            '--output', native(report), '--project', str(uuid.uuid4()), '--backend', 'auto',
            '--accept-runtime-license-and-experimental-ldr'], False)
        proof = json.loads((report / 'report.json').read_text())
        assert not any(proof['stages'].values()), proof
        assert 'MissingRuntime' in proof['error'], proof
        assert not list(report.glob('*.rgba')), 'failure exported an invented output'
        invalid = args.output / 'invalid-runtime'
        invalid.mkdir()
        (invalid / 'nvngx_dlssnr.dll').write_bytes(b'invalid PE fixture, never load')
        invalid_report = args.output / 'invalid-runtime-report'
        run('reject-invalid-pe-without-loading', ['--verify-runtime', '--runtime', native(invalid), '--backend', 'ngx', '--output', native(invalid_report)], False)
        proof = json.loads((invalid_report / 'report.json').read_text())
        assert proof['stages']['RUNTIME FOUND'] and not proof['stages']['RUNTIME VERIFIED'] and not proof['stages']['RUNTIME LOADED'], proof
        assert 'PE' in proof['error'], proof
    summary = {'neural_evaluation': False, 'environment': 'Wine' if args.wine else os.name,
               'input_source': 'synthetic CPU protocol fixture' if args.cpu_fixture else 'supplied captures', 'inputs': inputs, 'tests': results}
    (args.output / 'tests.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(f'{len(results)} CLI tests passed; no neural execution. Evidence: {args.output}')


if __name__ == '__main__':
    main()
