# `omsi-pack`

`omsi-pack` defines the opt-in `.oopp` directory format shared by openMapper and openOMSI.
An ordinary map remains a normal OMSI folder/archive. A publisher pack adds a signed manifest;
protected packs also split each asset into authenticated chunks.

## Manifest

`manifest.json` is canonical UTF-8 JSON in the declared `Manifest` field order. Its detached
`manifest.sig` is the 64-byte Ed25519 signature over those exact JSON bytes. The manifest names
the publisher and pack identity/version, author, licence, redistribution permission, optional
key ID, exact dependency versions, and each asset's stable ID, game path, original source-byte
SHA-256, converted output-byte SHA-256, size and chunk records. Trust is supplied separately by
the player in `TrustedPublishers`; a public key included by an untrusted pack is never trusted.

Publisher key IDs are the lowercase SHA-256 of the 32-byte Ed25519 public key. The player
rejects unsupported versions, non-canonical JSON, unknown fields, untrusted keys, unsafe or
case-colliding paths, duplicate identities, malformed hashes and bounded-size violations before
using pack data.

For a protected pack, the publisher assigns `key_id` as the lowercase SHA-256 of the fresh
32-byte AES key. Generate a new random content key for each new pack or version.

## Protected assets

`assets/<opaque-asset-id>/<chunk-number>.bin` stores one 12-byte random nonce followed by the
AES-256-GCM ciphertext and tag. Each chunk holds at most 64 KiB of plaintext. Its authenticated
metadata binds the publisher, pack and version, stable asset ID and path, chunk number/count, and
whole asset size. Signed ciphertext hashes are checked before authentication. A pack uses a fresh
random content key; chunk nonces are checked for duplicates within that pack before use.

An encrypted asset may be at most 64 MiB; a pack may contain at most 4096 assets, 256 exact
dependencies and 1 GiB of asset bytes. Each file is fully authenticated and checked against its
signed output hash before a decoder receives it. The runtime obtains the files through its sealed
virtual filesystem so the ordinary editor and texture-cache write guards still apply. Plaintext
exists in process memory and may reach GPU or operating-system paging; a modified player can
capture it. Encryption cannot prevent that on a computer that must render the asset.

Pack output is assembled in a private sibling directory and atomically renamed into place only
after the signed manifest is complete. An operating-system crash during creation can leave an
incomplete `.omsi-pack-stage-<random>.tmp` sibling; it is never accepted as a pack and can be
removed after the publisher tool has stopped.

## Offline entitlement

The publisher signs an entitlement binding the exact canonical manifest hash, publisher signing
key, pack ID/version, content-key ID and expiry. It contains the 32-byte pack key, so it is a
bearer secret: deliver the file confidentially. Anyone who copies it can unlock the same pack.
openOMSI stores the signed envelope in the macOS Keychain or user-scoped Windows DPAPI storage
and rechecks its signature, expiry and manifest binding whenever the pack opens. An expired or
invalid entitlement fails closed. Revocation cannot remove a key already unlocked for indefinite
offline playback.

The player does not contact a purchase service. The publisher decides who receives an
entitlement and may use its own trusted purchase or distribution process. Publisher signing
keys stay with the publisher; only public keys belong in the player's trust list.

## Producer and runtime API

The publisher creates a fresh `SigningKey`, chooses a new random content key when protection is
enabled, hashes source bytes before conversion, and passes the converted outputs and those source
hashes to `create_pack`. The returned public key is added to a trusted publisher list through an
explicit distribution channel. After a purchase decision, the publisher makes an entitlement with
`issue_entitlement` and transfers that bearer file confidentially.

The player imports the pack directory, verifies and unlocks an entitlement with `unlock`, and
mounts it with `VerifiedPack::open_unlocked`. The latter re-verifies every chunk before returning;
`VerifiedPack::read` releases one authenticated asset at a time. `VerifiedPack::open_for_authoring`
is reserved for the publisher's own tooling, which already holds its pack key.

This is format and cryptographic plumbing, not a claim that source code can stop a modified game
from copying content it has already decrypted.
