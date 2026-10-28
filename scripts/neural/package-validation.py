#!/usr/bin/env python3
"""Prepare a private offline validation kit. No NVIDIA DLL/SDK is copied.

A native NGX-linked host contains SDK object code: packaging it requires the
operator's separate confirmation of the applicable SDK/license rights.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import zipfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', type=Path, required=True)
    parser.add_argument('--capture-256', type=Path, required=True)
    parser.add_argument('--capture-512', type=Path, required=True)
    parser.add_argument('--capture-720p', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True, help='new directory; ZIP is created beside it')
    parser.add_argument('--wine', action='store_true', help='inspect our Windows build on Linux')
    parser.add_argument('--accept-sdk-binary-packaging', action='store_true')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    command = (['wine'] if args.wine else []) + [str(args.host.resolve()), '--build-info']
    info = json.loads(subprocess.check_output(command, text=True, timeout=60))
    if not info['windows_x64']:
        raise ValueError('Package requires our Windows x64 host')
    if info['ngx_compiled'] and not args.accept_sdk_binary_packaging:
        raise ValueError('NGX-linked SDK object code requires --accept-sdk-binary-packaging and appropriate rights; no public redistribution is implied')
    if args.output.exists() or args.output.with_suffix('.zip').exists():
        raise FileExistsError('Refusing to overwrite validation directory/ZIP')
    args.output.mkdir(parents=True)
    def copy(source, destination):
        target = args.output / destination
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
    copy(args.host, 'bin/openomsi-neural-host.exe')
    for source in sorted((root / 'tools/neural-host').iterdir()):
        if source.suffix in ('.cpp', '.hpp') or source.name == 'CMakeLists.txt':
            copy(source, 'tools/neural-host/' + source.name)
    for script in sorted((root / 'scripts/neural').glob('*')):
        if script.suffix in ('.ps1', '.py'):
            copy(script, 'scripts/neural/' + script.name)
    for doc in ['DLSS5_NEXT_ACTIONS.md', 'DLSS5_RUNTIME_VALIDATION.md', 'DLSS5_FINAL_VALIDATION_SOURCES.json']:
        copy(root / 'docs' / doc, 'docs/' + doc)
    copy(root / 'LICENSE', 'LICENSE')
    copy(root / '.github/workflows/neural-windows.yml', '.github/workflows/neural-windows.yml')
    captures = {}
    for name, source in [('256', args.capture_256), ('512', args.capture_512), ('720p', args.capture_720p)]:
        files = sorted(source.glob('*.omsi-nr'))
        if len(files) != 4:
            raise ValueError(f'{source}: expected four actual renderer captures')
        captures[name] = 'captures/' + name
        for file in files:
            copy(file, captures[name] + '/' + file.name)
        for extra in ['capture.json', 'verified-inputs.json']:
            if (source / extra).is_file():
                copy(source / extra, captures[name] + '/' + extra)
    config = {'schema': 1, 'host': 'bin/openomsi-neural-host.exe', 'host_build_info': info,
              'captures': captures, 'runtime_directory': None, 'contains_nvidia_runtime': False,
              'native_gpu_inference_validated': False}
    (args.output / 'validation.json').write_text(json.dumps(config, indent=2) + '\n')
    status = 'NGX-linked MSVC host, hardware unqualified' if info['ngx_compiled'] else 'DIAGNOSTIC ONLY: NGX is not linked; native MSVC build required for replay'
    (args.output / 'README.txt').write_text(status + '\n\nNo NVIDIA DLL is included. Read docs/DLSS5_NEXT_ACTIONS.md.\n'
        'Integrity only: ./scripts/neural/validate-bundle.ps1 -Runtime C:/PrivateNvidiaNR\n'
        'Replay additionally requires -Replay -Project <your-guid> -AcceptRuntimeLicenseAndExperimentalLdr -AcceptExperimentalNgx18\n'
        'Native MSVC binaries need the Microsoft Visual C++ x64 runtime. Never copy DLLs into Windows system folders.\n'
        'Host C++ sources are included: build-windows.ps1 can compile them with your authorized SDK.\n'
        'Rust/game builds additionally require the complete openOMSI repository.\n')
    manifest = {str(file.relative_to(args.output)).replace('\\', '/'): hashlib.sha256(file.read_bytes()).hexdigest()
                for file in sorted(args.output.rglob('*')) if file.is_file()}
    assert not any(Path(name).suffix.lower() == '.dll' for name in manifest)
    (args.output / 'SHA256.json').write_text(json.dumps(manifest, indent=2) + '\n')
    with zipfile.ZipFile(args.output.with_suffix('.zip'), 'w', zipfile.ZIP_DEFLATED) as archive:
        for file in sorted(args.output.rglob('*')):
            if file.is_file():
                archive.write(file, args.output.name + '/' + str(file.relative_to(args.output)))
    print(status)
    print('Package:', args.output.with_suffix('.zip'))


if __name__ == '__main__':
    main()
