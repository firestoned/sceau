<!--
Copyright (c) 2026 Erick Bourgeois, sceau
SPDX-License-Identifier: Apache-2.0
-->
# 0001 — TPM-sealed KMS v2 plugin

- **Status:** Accepted
- **Date:** 2026-09-04
- **Deciders:** Erick Bourgeois
- **Note:** Recorded retroactively — the implementation predates this record, per the ADD methodology (`ADR → CALM → TDD → implement → docs`) adopted at scaffold time.

## Context

k0s clusters on Kairos OS hosts need encryption of etcd data (Secrets) at
rest. The options all involve key management nobody wants to operate: the
built-in `aescbc`/`secretbox` providers need a plaintext key file on disk,
and an external KMS (Vault, cloud KMS) needs network infrastructure,
credentials, and lifecycle management for a small edge cluster.

Kairos hosts already have a TPM 2.0 and use it for measured boot / disk
encryption. A TPM is a hardware key store that never exports its root keys —
exactly the key-management-free root of trust this use case wants.

## Decision

sceau is a **Kubernetes KMS v2 gRPC plugin** (single static Rust binary) that
serves the apiserver over a unix socket (`/run/sceau/sceau.sock`, mode 0600)
and seals data encryption keys inside the TPM:

1. **SRK primary, deterministic** — at startup sceau recreates the standard
   RSA-2048 restricted decryption storage primary (the EK/SRK template:
   `fixedTpm`, `fixedParent`, `sensitiveDataOrigin`, empty auth) in the owner
   hierarchy via `TPM2_CreatePrimary`. Because the template is fully
   deterministic, the *same* primary key material is recreated on the same TPM
   after every reboot — no persistent state anywhere.
2. **Sealed DEKs** — `Encrypt` creates a keyed-hash sealed-data object
   (`fixedTpm` + `fixedParent`, null scheme) under the SRK containing the DEK;
   the returned ciphertext is an envelope of `version || public || private`
   blobs. `Decrypt` loads the blob back under the SRK and unseals it. Nothing
   secret exists outside the TPM in plaintext form.
3. **`key_id` from the SRK name** — the KMS `key_id` is derived from a hash of
   the SRK's Name (which binds the primary's public area), so it is stable per
   TPM across reboots. `Decrypt` rejects ciphertext tagged with any other
   `key_id`, which satisfies the KMS v2 contract after key rotation checks.
4. **k0s integration** — k0s runs the API server as a host process, so a plain
   host unix socket works; no sidecar or static pod. Deployment is a systemd
   unit on the Kairos host plus an `EncryptionConfiguration` pointing the
   apiserver at the socket.

## Consequences

**Positive**
- Zero key management: nothing to generate, store, back up, or distribute.
  The SRK never leaves the chip.
- Sealed objects are `fixedTpm` + `fixedParent`: they cannot be duplicated to
  another TPM; a cloned disk is useless without the same TPM.
- Deterministic primary = stateless plugin; survives OS reinstalls (Kairos
  A/B upgrades) as long as the TPM is not cleared.

**Negative / risks**
- **TPM loss = data loss.** A dead or cleared TPM (`tpm2_clear`, motherboard
  replacement) makes every sealed DEK permanently unrecoverable, and with it
  the etcd plaintext. This is accepted — the TPM is the root of trust — but
  cluster backups (etcd snapshots shipped elsewhere) become mandatory
  operational practice.
- Sealing is currently **possession-of-TPM only**: no PCR policy binding.
  **Deferred follow-up:** bind unseal to a PCR policy (e.g. PCR 7 secure-boot
  state or Kairos UKI measurements) so a disk moved to a machine that boots
  untrusted code will not unseal.
- RSA-2048 SRK create at every startup costs a few hundred milliseconds —
  acceptable for a daemon that starts once per boot.
- Throughput is bounded by the TPM (tens of ops/sec); fine for KMS workloads
  (one seal per Secret write) but not for bulk data.

## Alternatives considered

- **Plaintext key file (`aescbc`)** — rejected: the key sits on the same disk
  as the data it protects.
- **External KMS (Vault / cloud)** — rejected: network dependency and
  credential management are exactly what edge k0s clusters cannot assume.
- **Persist an NV-index primary instead of recreating from template** —
  rejected for now: NV state survives `tpm2_clear` differently across vendors
  and adds lifecycle (provisioning) steps; the deterministic template is
  stateless. May be revisited alongside PCR policy binding.
