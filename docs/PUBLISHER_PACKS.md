# Publisher packs

Publisher packs are an optional way to ship content with a signed record of who published it,
exact dependencies between maps and packs, and, if wanted, encrypted assets. This page is for
publishers who sell or distribute content, and for players who install such packs.

Ordinary maps and mods keep their normal formats and need none of this. Publisher packs need no
online service.

## How it works

1. The publisher creates an Ed25519 key pair and keeps the private key.
2. The publisher builds a pack: a `.oopp` folder with a signed manifest (`manifest.json` and
   `manifest.sig`) and, for a protected pack, encrypted assets.
3. The player adds the publisher's public key to `Publishers.json` in the content folder,
   obtained through a channel they trust.
4. The player installs the pack under `Packs` in the content folder.
5. A map that uses the pack lists it in its `dependencies.json`.
6. For a protected pack, the player also installs an entitlement from the publisher.

A trusted signature identifies the configured signer. It does not prove that the signer owns the
content. Importing a pack never adds the key it carries to `Publishers.json`: the player adds keys
themselves.

## Files

| File | Where | What it is |
|---|---|---|
| `Publishers.json` | Content folder | The publishers' public keys the player trusts |
| `Packs/<name>.oopp/` | Content folder | An installed pack |
| `manifest.json`, `manifest.sig` | In the pack | The signed hash list |
| `dependencies.json` | Beside a map's `global.cfg` | The packs the map needs |
| `PackIndex/` | Content folder | Verified manifest snapshots kept by tools (see [Details](#details)) |

## Map dependencies

A map declares the packs it needs, and which of its files come from them, in
`dependencies.json` beside `global.cfg`:

```json
{
  "format_version": 1,
  "dependencies": [
    {"publisher_id": "example", "pack_id": "scenery", "version": "1.0.0"}
  ],
  "assets": [
    {
      "path": "Sceneryobjects/MyMap/sign.sco",
      "dependency": {"publisher_id": "example", "pack_id": "scenery", "version": "1.0.0"},
      "asset_id": "sign"
    }
  ]
}
```

- Versions are exact requirements.
- A missing, wrong-version, untrusted or changed pack is a loading error.
- An installed pack is not a content root. Only maps that declare it in `dependencies.json` see
  its files. A pack never replaces a file of the game or of a mod, and never supplies a file that
  another map is missing.
- Map references are editable map data, not signed publisher instructions. Their paths are
  limited to normal game data paths and never make code trusted.
- A rights declaration in an editor export does not create a key or make a signature valid.

## The manifest

`manifest.json` is canonical UTF-8 JSON. `manifest.sig` is the Ed25519 signature over its exact
bytes, so whitespace, field order and every value matter. The manifest names the publisher, the
pack and its version, the author, the licence, whether redistribution is allowed, the exact
dependencies, and each asset:

| Field | Meaning |
|---|---|
| `id` | Stable identity assigned by the publisher |
| `path` | Game path of the playable file |
| `source_sha256` | SHA-256 of the original source bytes, before conversion |
| `output_sha256` | SHA-256 of the converted playable bytes |
| `size` | Output length in bytes |
| `chunks` | Protected chunk locations, lengths and ciphertext hashes |

- Websites can compare these hashes without a content key. No website, Workshop or bot
  integration is included.
- Hashes are of exact bytes. They survive renaming, but any change to geometry, a texture or a
  file gives a different hash.
- The game rejects unsupported versions, non-canonical JSON, unknown fields, untrusted keys,
  unsafe or case-colliding paths, duplicates, bad hashes and oversized data before using a pack.

## Protected packs

A protected pack encrypts its assets:

- Each pack (and each new version) has a new random AES-256-GCM content key.
- Assets are split into authenticated chunks of at most 64 KiB, with random nonces and checked
  chunk order.
- Every asset is authenticated and checked against its signed hash before it is decoded.
- Assets are read from memory. The editor and the texture cache do not write sealed content to
  disk.

| Limit | Value |
|---|---|
| One encrypted asset | 64 MiB |
| Assets per pack | 4096 |
| Dependencies per pack | 256 |
| Asset bytes per pack | 1 GiB |
| Signed manifest | 16 MiB |

### Entitlements

To play a protected pack, the player needs an **entitlement**: a file signed by the publisher for
one exact manifest, with an expiry date. It contains the pack's key.

- Deliver entitlements confidentially. Anyone who copies one can unlock the same pack.
- The game stores it in the macOS Keychain or in Windows user-scoped DPAPI storage, with no
  plaintext fallback. It checks signature, expiry and manifest whenever the pack opens. An expired
  or invalid entitlement fails closed.
- Encrypted packs therefore play only on **Windows and macOS**. Linux, Android and iOS have no key
  store yet and load unencrypted packs only.
- Any program of the user can read the Keychain item, so the signed game, its launcher and their
  updates read it without a prompt. macOS still asks for unsigned builds and builds signed without
  a team (local and CI builds).
- A new version or a changed manifest needs a new entitlement.
- Copying the application settings does not back up keys. If the key store cannot be restored,
  get a new entitlement from the publisher.

### One pack per sale

A publisher can build a separate protected pack for each sale. Its manifest then carries an
opaque `licence_id`, and only an entitlement with the same ID unlocks it.

- A leaked pack or entitlement shows which sale it came from.
- Packs and entitlements of different buyers are not interchangeable.
- The ID holds no personal data. Only the publisher's own records link it to a buyer.
- The ID is 1-128 ASCII letters, digits, dots, underscores or hyphens, for example 32 random
  hexadecimal characters.
- Shared packs have no `licence_id`. Their manifests and entitlements are unchanged.

### What protection cannot do

- The game does not contact any purchase service. The publisher decides who gets an entitlement.
- Offline play cannot enforce immediate revocation, or stop a user from turning the system clock
  back.
- A running game holds the decrypted data in memory and on the GPU. Paging or a modified program
  can expose it.

Protected packs reduce accidental redistribution through the official tools. They do not stop
extraction by someone who controls the computer.

## The omsi-pack tool

Publishers and players use the `omsi-pack` command-line tool from the openOMSI repository:

| Command | What it does |
|---|---|
| `omsi-pack keygen <signing.pk8> <Publishers.json> <publisher-id>` | Creates a signing key and a trust file with its public key. Refuses to overwrite existing files. |
| `omsi-pack issue <pack.oopp> <signing.pk8> <pack-key.bin> <expires-unix> <entitlement.json> [--licence-id <id>]` | Issues an entitlement. A per-sale pack needs `--licence-id`, matching the manifest. |
| `omsi-pack unlock <pack.oopp> <entitlement.json> <Publishers.json>` | Verifies an entitlement and stores it in the system key store |
| `omsi-pack inspect <pack.oopp> <Publishers.json>` | Verifies a pack and shows its contents |

Only public keys belong in a player's `Publishers.json`. Signing keys stay with the publisher.

The byte-level pack format is described in `crates/omsi-pack/README.md` in the repository.

## Details

- Recognising which files of a map came from a pack (provenance) is a library feature for later
  tools. The game, the launcher and the editor do not use it yet: nothing writes `PackIndex` while
  playing, and exporting a map does not warn about publisher content.
- A tool that uses it keeps verified manifest snapshots in `PackIndex`. Each snapshot is checked
  against the current `Publishers.json` when used, so removing a key also removes recognition of
  that publisher's files.
- Installed revisions are read first. History past 1024 snapshots, 64 MiB of manifests or 32768
  assets is skipped with a warning. An installed revision that does not fit still fails closed. A
  snapshot left half-written by an interrupted write is ignored.
- An interrupted pack build can leave a `.omsi-pack-stage-<random>.tmp` folder beside the pack.
  It is never accepted as a pack and can be deleted.
- The map layer cache is not written for a map with `dependencies.json`, nor for a map whose
  layers were made while files of an encrypted pack were read. Other maps keep their cache, also
  with encrypted packs installed.
- Multiplayer session stores are sealed at rest but hold no publisher-protected content.
