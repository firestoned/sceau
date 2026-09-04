# TPM Sealing

Everything sceau does reduces to two TPM 2.0 operations: create a sealed-data
object under the Storage Root Key (SRK), and unseal it later. This page
describes the object model — the SRK template, the sealed-object attributes,
and the envelope format — as implemented in
[`src/tpm.rs`](https://github.com/firestoned/sceau/blob/main/src/tpm.rs) and
recorded in
[ADR-0001](https://github.com/firestoned/sceau/blob/main/docs/adr/0001-tpm-sealed-kms-v2-plugin.md).

## The SRK primary: deterministic by design

At startup sceau calls `TPM2_CreatePrimary` in the **owner hierarchy** with
the standard SRK template:

| Template property | Value |
| --- | --- |
| Algorithm | RSA-2048 |
| Name alg | SHA-256 |
| Type | restricted decryption key |
| Symmetric | AES-128-CFB |
| Attributes | `fixedTpm`, `fixedParent`, `sensitiveDataOrigin`, `userWithAuth` |
| Auth | empty |

The decisive property: **primary key creation is deterministic.** A TPM
derives a primary from its internal seed plus the template; the same template
on the same TPM always yields the same key material. sceau therefore:

- stores **no state** — no key file, no NV index, no database;
- recreates the identical SRK after every reboot, daemon restart, or Kairos
  A/B OS upgrade;
- pays a few hundred milliseconds of RSA-2048 key generation once per boot —
  irrelevant for a daemon that starts once.

The primary is flushed from TPM memory when sceau exits (`Drop` flushes the
handle), so nothing lingers in the chip's volatile memory either.

## Sealed-data objects

`Encrypt` does not encrypt the DEK with the SRK directly (RSA can only cover
a symmetric key's worth of bytes). Instead it creates a **keyed-hash
sealed-data object** under the SRK via `TPM2_Create`:

| Property | Value |
| --- | --- |
| Public algorithm | `KeyedHash` (null scheme) |
| Name alg | SHA-256 |
| Attributes | `fixedTpm`, `fixedParent`, `userWithAuth` |
| Sensitive payload | the DEK bytes (max 128 bytes — `TPM2B_SENSITIVE_DATA` cap) |

`TPM2_Create` returns two blobs: the object's **public area** and its
encrypted **private area**. The private area can only be decrypted by the TPM
whose SRK parented it. Those two blobs, together, *are* the ciphertext.

### What fixedTpm + fixedParent buy you

- **`fixedTpm`** — the object can never be duplicated to a different TPM.
  There is no TPM command that exports it in a migratable form.
- **`fixedParent`** — the object can never be re-parented away from the SRK
  that created it.

Together: a sealed DEK is usable **only on the TPM that sealed it, under the
SRK that parented it**. Copy the disk, clone the etcd snapshot, image the
machine — the envelopes are inert without the original chip.

## The envelope format

The KMS ciphertext is a small self-describing envelope:

```text
offset  field       size              meaning
0       version     1 byte            envelope format version (currently 1)
1       public_len  2 bytes, big-end. length of the marshalled public area
3       public      public_len bytes  TPM2B_PUBLIC (marshalled tss-esapi Public)
3+n     private     remaining bytes   TPM2B_PRIVATE (encrypted private area)
```

`Decrypt` reverses it: validate the version byte, slice out the public area,
unmarshal both blobs, `TPM2_Load` the object under the SRK, `TPM2_Unseal`,
and `TPM2_FlushContext` the loaded handle. Any structural deviation — wrong
version, truncated length, unmarshalling failure — is rejected as a malformed
envelope before the TPM is touched.

## key_id derivation

A TPM object's **Name** is the hash of its public area (algorithm ID ||
digest), so it cryptographically binds the SRK template. sceau derives the
KMS `key_id` from it at startup:

```text
key_id = "sceau-" + hex(SHA-256(SRK Name))[0..16]
```

Consequences:

- **Stable per TPM** — same template, same chip ⇒ same Name ⇒ same `key_id`,
  across reboots and reinstalls.
- **Distinct across TPMs** — each chip's seed differs, so no two hosts share
  a `key_id`. A `Decrypt` that arrives with another host's `key_id` (or any
  other value) is rejected with `INVALID_ARGUMENT`.
- **Not secret** — the Name is derived from the *public* area; logging it is
  safe, and sceau logs it at startup.

## What is deliberately not here

- **No PCR policy** — unsealing requires possession of the TPM, nothing else.
  Binding unseal to boot measurements (PCR 7, Kairos UKI) is on the roadmap;
  see the [Threat Model](threat-model.md).
- **No NV-index persistence** — the deterministic template makes persistent
  TPM state unnecessary, and NV semantics under `tpm2_clear` vary by vendor.
- **No auth values** — both the SRK and sealed objects use empty auth; the
  unix socket's mode `0600` is the access control, not TPM passwords.
