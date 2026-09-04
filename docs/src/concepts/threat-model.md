# Threat Model

What TPM sealing does and does not protect against, stated plainly. The
security posture is a set of deliberate trade-offs recorded in
[ADR-0001](https://github.com/firestoned/sceau/blob/main/docs/adr/0001-tpm-sealed-kms-v2-plugin.md) —
this page is the operator-facing summary. To report a vulnerability, see
[Security](../reference/security.md).

## Protected assets

- The **plaintext of etcd Secrets** at rest.
- The **data encryption keys (DEKs)** that encrypt those Secrets.
- The **root key material** (the SRK), which exists only inside the TPM.

## What sceau protects against

| Attack | Defence |
| --- | --- |
| **Disk theft / disk imaging** — attacker copies the etcd data directory or clones the drive. | Sealed objects are `fixedTpm` + `fixedParent`: useless without the original TPM. The clone yields AES-GCM ciphertext and inert envelopes. |
| **Key-file theft** — attacker reads the KMS key from disk, as with `aescbc`/`secretbox` providers. | There is no key file. The SRK is recreated in TPM-internal memory at startup and never touches persistent storage. |
| **Post-mortem forensic recovery** — attacker extracts keys from swap, crash dumps, or decommissioned media. | DEKs exist in plaintext only transiently in apiserver memory; the SRK never exists outside the chip. |
| **Offline brute force of envelopes** | Envelope private areas are protected by the TPM's seed-derived hierarchy keys — there is no password to guess and no offline oracle. |
| **Unprivileged local process asking sceau to unseal** | The KMS socket is created mode `0600`; only root (the apiserver) can connect. |

## The explicit trade-off: TPM loss = data loss

This is the design's centre of gravity, and it is **intentional**:

!!! failure "Treat the TPM as the root of trust it is"
    A dead TPM, a motherboard replacement, or `tpm2_clear` makes every sealed
    DEK **permanently unrecoverable** — and with it, the plaintext of every
    encrypted Secret in etcd. There is no recovery path, no escrow, no backup
    key. That is what "no key management" means: there is nothing to back up,
    and therefore nothing to restore from.

Operational consequences:

- **etcd snapshots shipped off-host are mandatory.** A snapshot is only as
  useful as your ability to decrypt it — either restore onto the *same* TPM,
  or ensure you also have a disaster-recovery path that does not depend on
  the sealed DEKs (e.g. rebuilding cluster state from GitOps source).
- **Do not clear the TPM** as part of unrelated maintenance (firmware
  updates sometimes prompt for this) without first migrating etcd encryption
  to a different provider.
- **Motherboard swaps are cluster rebuilds** unless the TPM is a discrete
  module that moves with the board replacement — plan accordingly.

## Current limitation: possession-of-TPM only

Today, unsealing requires only **physical possession of the functioning
TPM** — no PCR policy is checked. That means:

!!! warning "Disk + machine moved together will unseal"
    If an attacker steals the *whole machine* (or moves the disk to a machine
    whose boot path they control while keeping the original TPM), the TPM
    will happily unseal under its current policy. The seal binds to the chip,
    not to the software state of the boot.

**Roadmap:** bind unseal to a PCR policy — e.g. PCR 7 (secure-boot state) or
the Kairos UKI measurements — so a host booted into untrusted code refuses to
unseal. This is tracked as a follow-up in ADR-0001 and will be designed
through the ADD process (ADR → CALM → TDD) like every other architectural
change.

## Trust boundaries and out of scope

The trust model has one boundary: **root on the host**.

- The KMS socket is mode `0600` precisely because root *is* the boundary.
  Anyone with root can ask sceau to unseal anything — issues that presuppose
  root access are out of scope for vulnerability reports.
- The TPM is trusted to be genuine (no interposer on the LPC/SPI bus), and
  the boot chain is currently trusted implicitly (see the PCR roadmap above).
- sceau serves gRPC over a unix socket only. There is no network listener, so
  remote attacks on the KMS protocol itself are not possible — a remote
  attacker must first become a local root process.

## Supply chain

Because sceau touches every DEK the cluster issues, released artifacts carry
the verification set described in
[ADR-0002](https://github.com/firestoned/sceau/blob/main/docs/adr/0002-release-and-supply-chain-pipeline.md):
CycloneDX SBOM on releases, Cosign keyless signatures on pushed images, Trivy
scanning, SHA-pinned GitHub Actions, and `cargo-deny` / `cargo-audit` gates
in CI. Verify before you deploy:

```sh
cosign verify ghcr.io/firestoned/sceau@<digest> \
  --certificate-identity-regexp 'https://github.com/firestoned/sceau/.*' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```
