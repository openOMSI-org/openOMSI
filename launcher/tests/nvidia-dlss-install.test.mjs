// Prevent accidental NVIDIA redistribution in CI and regressions in consent/integrity checks.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const file = (relative) => readFileSync(new URL(relative, import.meta.url), 'utf8');
const release = file('../../.github/workflows/release.yml');
const packager = file('../../scripts/windows/package-streamline.ps1');
const installer = file('../../scripts/windows/install-streamline.ps1');
const backend = file('../src-tauri/src/nvidia_dlss.rs');
const commands = file('../src-tauri/src/commands.rs');
const frontend = file('../ui/js/pages/settings.js');
const schema = file('../ui/js/pages/settings-schema.js');
const api = file('../ui/js/api.js');

test('no NVIDIA DLLs are bundled by Windows release workflow', () => {
  assert.doesNotMatch(release, /nvngx|sl\.interposer|Streamline/i);
  assert.match(release, /AMD FidelityFX SDK 2\.4\.0/);
});

test('player is shown NVIDIA terms and must explicitly opt in', () => {
  assert.match(schema, /id: 'nvidia_dlss_install'.*windows/);
  assert.match(frontend, /NVIDIA_NGX_LICENSE/);
  assert.match(frontend, /NVIDIA_STREAMLINE_LICENSE/);
  assert.match(frontend, /I have read and agree to NVIDIA/);
  assert.match(frontend, /disabled=\$\{busy \|\| !accepted/);
  assert.match(frontend, /acceptedTerms: true/);
  assert.match(backend, /if !accepted_terms/);
  assert.match(commands, /fn nvidia_dlss_install\(accepted_terms: bool\)/);
  assert.match(api, /'nvidia_dlss_install'/);
});

test('installer downloads only official Streamline and verifies SDK hash and signatures', () => {
  assert.match(installer, /https:\/\/github\.com\/NVIDIA-RTX\/Streamline\/releases\/download\/v2\.14\.1\/streamline-sdk-v2\.14\.1\.zip/);
  assert.match(installer, /-MaximumRedirection 5/);
  assert.match(installer, /& \$PackagingScript -Archive \$Archive -Destination \$Stage/);
  assert.match(packager, /92c4d954631a1710da86ca3fa8d5034f2b9503838c95fc4ae977ae149319781b/);
  assert.match(packager, /Get-AuthenticodeSignature/);
  assert.match(packager, /\$Sig\.Status -ne "Valid"/);
  assert.match(packager, /O=NVIDIA Corporation/);
  assert.match(backend, /System32.*powershell\.exe/);
  assert.match(installer, /Move-Item -LiteralPath \$BackupFile/);
  for (const name of ['sl.interposer.dll', 'sl.common.dll', 'sl.dlss.dll', 'nvngx_dlss.dll']) {
    assert.ok(installer.includes(name), `installer needs ${name}`);
    assert.ok(backend.includes(name), `backend needs ${name}`);
  }
  assert.match(installer, /nvngx_dlss\.license\.txt/);
});

test('installer is Windows only and checks RTX and the game folder', () => {
  assert.match(backend, /#\[cfg\(windows\)\]/);
  assert.match(backend, /omsi_cfg::upscaler_files::rtx_present\(\)/);
  assert.match(backend, /openomsi\.exe/);
  assert.match(installer, /-notmatch '--launcher-service'/);
  assert.match(installer, /Close openOMSI before installing/);
});
