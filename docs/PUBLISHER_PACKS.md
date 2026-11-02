# Optional publisher packs

Ordinary maps and assets keep their current formats. Publisher packs add opt-in provenance,
dependency references and, separately, protected playback. They require no hosted service.

A publisher signs a manifest with its Ed25519 private key. Players explicitly configure the
publisher's public key in `Publishers.json` in their content folder. Importing a pack must never
add its supplied key to that trust file automatically. A trusted signature identifies the
configured signer; it does not establish ownership of the content.

## Installed layout and dependencies

Each installed `.oopp` directory goes under the content folder's `Packs` directory. The public
manifest and detached signature describe the pack's exact version, licence and redistribution
policy. `PackIndex` keeps verified signed manifest snapshots for provenance recognition. Each
snapshot is checked against the current trust file when used; removing all trusted records
also removes the ability to recognize those source bytes.

A map may carry `dependencies.json` beside `global.cfg`:

```json
{"format_version":1,"dependencies":[{"publisher_id":"example","pack_id":"scenery","version":"1.0.0"}],"assets":[{"path":"Sceneryobjects/MyMap/sign.sco","dependency":{"publisher_id":"example","pack_id":"scenery","version":"1.0.0"},"asset_id":"sign"}]}
```

Versions are exact requirements. Missing, wrong-version, untrusted or changed packs produce
loader errors. A rights declaration in an editor export does not create a decryption key or
make a signature valid. Map references are editable map data, not authenticated publisher
instructions; their aliases are limited to compatible game data paths and never confer code trust.

## Public hash list

The version 1 published hash-list format is `manifest.json` together with `manifest.sig`,
defined by [`Manifest`](../crates/omsi-pack/src/lib.rs) and the
[pack format](../crates/omsi-pack/README.md). Its asset records contain:

| Field | Meaning |
|---|---|
| `id` | Stable identity assigned by the publisher |
| `path` | Relative game path of the playable output |
| `source_sha256` | SHA-256 of the original source bytes before conversion |
| `output_sha256` | SHA-256 of the converted playable bytes |
| `size` | Output length in bytes |
| `chunks` | Protected chunk locations, lengths and ciphertext hashes |

The signature covers the exact canonical UTF-8 manifest bytes. Whitespace, field order and
all values are significant. Consumers must use an independently obtained trusted public key.
Sites can compare these hashes without receiving a content key. No website, Workshop or bot
integration is included. Exact byte hashes survive renaming; arbitrary geometry, texture or
file changes produce different hashes.

## Protected playback and limits

The optional protected format uses a new random AES-256-GCM content key per exported pack,
random nonces, authenticated chunk identity/order, and a signed manifest. Bounds and byte
layouts are in the pack-format document. Authentication completes before asset decoding.
Official playback uses memory-backed virtual reads and preserves the existing sealed-content
editor and texture-cache restrictions.

Publishers issue signed offline entitlements for an exact manifest and expiry. Entitlements
contain a pack key and must be delivered confidentially. Runtime storage uses macOS Keychain
or Windows user-scoped DPAPI, without a plaintext fallback. A new version or changed manifest
requires a corresponding entitlement. Copying application settings is not a supported key
backup; obtain another entitlement from the publisher when native storage cannot be restored.

Offline issuance requires a trusted publisher decision. The client cannot independently
discover purchases, and a client-controlled ownership flag is never proof of purchase.
Offline operation cannot enforce immediate revocation or prevent deliberate system-clock
rollback by a user controlling the machine. A running player also holds usable plaintext in
memory and GPU resources; paging and a modified executable can expose it. Protected packs
reduce accidental redistribution through official tools, not extraction by an administrator.
