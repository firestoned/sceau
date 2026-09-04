<!-- Copyright (c) 2026 Erick Bourgeois, sceau -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# sceau

[![Build](https://github.com/firestoned/sceau/actions/workflows/build.yaml/badge.svg?branch=main)](https://github.com/firestoned/sceau/actions/workflows/build.yaml)
[![Documentation](https://github.com/firestoned/sceau/actions/workflows/docs.yaml/badge.svg?branch=main)](https://github.com/firestoned/sceau/actions/workflows/docs.yaml)
[![SAST](https://github.com/firestoned/sceau/actions/workflows/sast.yaml/badge.svg?branch=main)](https://github.com/firestoned/sceau/actions/workflows/sast.yaml)
[![CodeQL](https://github.com/firestoned/sceau/actions/workflows/codeql.yaml/badge.svg?branch=main)](https://github.com/firestoned/sceau/actions/workflows/codeql.yaml)
[![OpenSSF Scorecard](https://api.scorecard.dev/projects/github.com/firestoned/sceau/badge)](https://scorecard.dev/viewer/?uri=github.com/firestoned/sceau)

[![License](https://img.shields.io/github/license/firestoned/sceau?color=blue)](LICENSE)
[![Docs](https://img.shields.io/badge/docs-firestoned.github.io%2Fsceau-blue)](https://firestoned.github.io/sceau/)
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange?logo=rust)](https://www.rust-lang.org/)
[![Status](https://img.shields.io/badge/status-In%20Development-orange)](#status)
[![Issues](https://img.shields.io/github/issues/firestoned/sceau)](https://github.com/firestoned/sceau/issues)
[![Last commit](https://img.shields.io/github/last-commit/firestoned/sceau/main)](https://github.com/firestoned/sceau/commits/main)
[![PRs welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/firestoned/sceau/pulls)

*sceau* (French for "seal") is a [Kubernetes KMS v2](https://kubernetes.io/docs/tasks/administer-cluster/kms-provider/)
plugin that encrypts etcd data at rest by sealing data encryption keys (DEKs)
directly with a TPM 2.0. There are no keys to generate, store, rotate, or back
up — the TPM's storage root key never leaves the chip.

Designed for [Kairos](https://kairos.io) hosts running [k0s](https://k0sproject.io).

**Full documentation: <https://firestoned.github.io/sceau/>** — concepts,
guides (quickstart, k0s setup, Kairos deployment, internal registry), and
reference. The sections below are the quick reference; build the site locally
with `make docs-serve`.

## How it works

Kubernetes envelope encryption: the API server generates a DEK per write and
sends it to this plugin over a unix socket. `sceau` seals the DEK inside the
TPM (under an RSA-2048 restricted decryption primary recreated from the
standard SRK template at startup) and returns the sealed blob as the KMS
ciphertext. Decrypt loads the blob back into the TPM and unseals it.

```
kube-apiserver ──unix socket──> sceau ──/dev/tpmrm0──> TPM 2.0
   (KMS v2 gRPC)                 (seal/unseal)          (SRK, never exported)
```

Because the SRK template is deterministic, the same primary key is recreated
after every reboot on the same TPM — ciphertexts survive restarts with zero
persistent state. Move or reset the TPM and sealed data is unrecoverable,
which is the point.

The `key_id` reported to the API server is derived from the SRK name, so it is
stable per TPM. `Decrypt` rejects ciphertext tagged with any other key.

## Build

Requires the TPM2 TSS stack (`tpm2-tss` development libraries), Rust, and
`protoc`:

```sh
cargo build --release
```

The binary is a single static-ish executable — trivial to ship into a Kairos
image.

## Run

```sh
sceau --socket /run/sceau/sceau.sock --tcti device:/dev/tpmrm0
```

## k0s configuration

k0s runs the API server as a host process, so a plain unix socket on the host
works — no sidecar or static pod needed.

`/var/lib/k0s/encryption.conf`:

```yaml
apiVersion: apiserver.config.k8s.io/v1
kind: EncryptionConfiguration
resources:
  - resources: ["secrets"]
    providers:
      - kms:
          apiVersion: v2
          name: sceau
          endpoint: unix:///run/sceau/sceau.sock
          timeout: 3s
      - identity: {}
```

`k0s.yaml`:

```yaml
spec:
  api:
    extraArgs:
      encryption-provider-config: /var/lib/k0s/encryption.conf
```

Keep `identity: {}` as a fallback provider until the first write has been
encrypted, then follow the standard KMS migration procedure
(`kubectl get secrets --all-namespaces -o json | kubectl replace -f -`).

## Kairos deployment

Example systemd unit:

```ini
[Unit]
Description=sceau KMS plugin (TPM)
Before=k0scontroller.service

[Service]
RuntimeDirectory=sceau
RuntimeDirectoryMode=0700
ExecStart=/usr/local/bin/sceau --socket /run/sceau/sceau.sock
Restart=always
RestartSec=2

[Install]
WantedBy=multi-user.target
```

## Security notes

- The KMS socket is created with mode `0600`; only root (and thus the API
  server) can talk to it.
- Sealed objects are `fixedTpm` + `fixedParent`: they cannot be duplicated to
  another TPM.
- Roadmap: bind unseal to a PCR policy (e.g. PCR 7 / secure boot state, or
  Kairos UKI measurements) so disks moved to an unlocked-boot machine will not
  unseal. Currently the seal is possession-of-TPM only.
- Loss of the TPM (or `tpm2_clear`) means loss of etcd plaintext. Treat the
  TPM as the root of trust it is.

## Status

Early skeleton: KMS v2 gRPC server + TPM seal/unseal are implemented; PCR
policy binding, graceful SRK eviction under memory pressure, and e2e tests
against a swtpm are next.
