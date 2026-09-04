# sceau

> A Kubernetes **KMS v2** plugin that encrypts etcd data at rest by sealing
> data encryption keys directly with a **TPM 2.0**. No keys to generate, store,
> rotate, or back up — the TPM's storage root key never leaves the chip.

[![Build](https://github.com/firestoned/sceau/actions/workflows/build.yaml/badge.svg?branch=main)](https://github.com/firestoned/sceau/actions/workflows/build.yaml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](reference/license.md)
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange.svg?logo=rust)](https://www.rust-lang.org/)
[![Status](https://img.shields.io/badge/status-In%20Development-orange.svg)](https://github.com/firestoned/sceau)

---

## What is sceau?

**sceau** (French for "seal") is a [Kubernetes KMS v2](https://kubernetes.io/docs/tasks/administer-cluster/kms-provider/)
gRPC plugin written in Rust. When kube-apiserver writes a Secret, it generates a
data encryption key (DEK), encrypts the Secret with it, and hands the DEK to
sceau over a unix socket. sceau **seals the DEK inside the host's TPM 2.0** and
returns the sealed blob as the KMS ciphertext. On read, the blob is loaded back
into the TPM and unsealed.

The root of trust is the TPM's Storage Root Key (SRK) — recreated
deterministically from the standard RSA-2048 template at every startup, never
exported, never on disk.

```text
kube-apiserver ──unix socket──> sceau ──/dev/tpmrm0──> TPM 2.0
   (KMS v2 gRPC)                 (seal/unseal)          (SRK, never exported)
```

Designed for [Kairos](https://kairos.io) hosts running [k0s](https://k0sproject.io),
where the API server runs as a host process and a plain host unix socket — mode
`0600`, no sidecar, no static pod — is all the plumbing needed.

## Why does sceau exist?

Because every other answer to "encrypt etcd at rest on a small edge cluster"
involves **key management nobody wants to operate**:

- The built-in `aescbc` / `secretbox` providers need a **plaintext key file on
  disk** — on the same disk as the data it protects.
- An external KMS (Vault, a cloud KMS) needs **network infrastructure,
  credentials, and lifecycle management** — exactly what an edge k0s cluster
  cannot assume.

Kairos hosts already carry a TPM 2.0 and use it for measured boot and disk
encryption. A TPM is a hardware key store that never exports its root keys:
precisely the key-management-free root of trust this use case wants. See
[ADR-0001](https://github.com/firestoned/sceau/blob/main/docs/adr/0001-tpm-sealed-kms-v2-plugin.md)
for the full decision record.

## The pitch: nothing to manage

- **No keys to generate** — the SRK primary is recreated from a deterministic
  template at startup; the same TPM always produces the same primary.
- **No state to back up** — sealed DEK envelopes live in etcd; sceau itself is
  fully stateless and survives OS reinstalls (Kairos A/B upgrades).
- **No duplication** — sealed objects are `fixedTpm` + `fixedParent`; they
  cannot be duplicated to another TPM. A cloned disk is useless without the
  same chip.
- **A stable identity** — the KMS `key_id` derives from the SRK name, so it is
  stable per TPM across reboots.

The trade-off is explicit: **lose the TPM, lose the data**. That is the point of
a root of trust — see the [Threat Model](concepts/threat-model.md).

## Project status

sceau is **early**. The KMS v2 gRPC server and TPM seal/unseal are implemented;
PCR policy binding, graceful SRK eviction under memory pressure, and end-to-end
tests against a swtpm are next. Don't run production workloads against it yet.

## Where to go next

- [Overview — how envelope encryption, KMS v2, and TPM sealing fit together](overview.md) ← start here
- [Quickstart — build and run against a local swtpm](guides/quickstart.md)
- [k0s Setup — EncryptionConfiguration and migration](guides/k0s-setup.md)
- [Kairos Deployment — systemd unit and image install](guides/kairos-deployment.md)
- [Concepts — KMS v2, TPM sealing, threat model](concepts/index.md)
- [Developer — local development and the ADD workflow](developer/index.md)

## Community & support

- **GitHub Issues**: <https://github.com/firestoned/sceau/issues>
- **Security reports**: see the [Security](reference/security.md) page — please
  do not open public issues for vulnerabilities.

## License

sceau is open-source software, licensed under the [Apache License 2.0](reference/license.md).
