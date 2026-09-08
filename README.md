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

### Container image

`make docker-image` builds the distroless container image from a pre-built
Linux binary (see `Makefile` for the full `build-linux-*` staging logic —
never `cargo build` inside the image build):

```sh
make docker-image ARCH=amd64 PUSH=true \
    REGISTRY=registry.example.com \
    ORG=my-org \
    IMAGE_TAG=v0.1.0
```

`build-linux-*` prefers a host gcc cross-toolchain (or native compile on a
matching Linux host); it only falls back to a container (`RUST_BUILD_IMAGE`,
default `rust:1-bookworm`) when `libtss2-dev` isn't linkable for the target
triple on the host — see ADR-0002.

#### Restricted networks (private registry / package mirrors only)

On networks that block Docker Hub, `deb.debian.org`, and `crates.io` directly
(only an internal artifact mirror is reachable), override:

| Variable | Purpose |
| --- | --- |
| `BASE_IMAGE` | distroless runtime base, e.g. `registry.example.com/distroless/cc-debian13:nonroot`. Unset by default, in which case the digest-pinned `FROM` in the `Dockerfile` is used. |
| `RUST_BUILD_IMAGE` | build container for the `build-linux-*` fallback, e.g. `registry.example.com/rust:1-bookworm` |
| `APT_SETUP_CMD` | shell command `eval`'d inside `RUST_BUILD_IMAGE` before `apt-get update` — see below |
| `CA_BUNDLE` | host path to a CA cert/bundle file, mounted into the container and trusted via `update-ca-certificates` before apt/cargo run — see below |
| `CARGO_REGISTRY_MIRROR` | Cargo sparse-index mirror URL, e.g. `sparse+https://registry.example.com/api/cargo/rust-remote/index/` |
| `REGISTRY` / `ORG` | where the built image is pushed |

`APT_SETUP_CMD` is a raw hook, not a templated hostname substitution — a real
private-mirror apt setup typically needs a custom repo path and a separate
security repo, possibly with proxy overrides. Build it in a heredoc so
quoting stays sane, then pass it through:

```sh
APT_SETUP_CMD=$(cat <<'EOF'
rm -f /etc/apt/sources.list.d/*.sources
echo "deb https://registry.example.com/debian trixie main" > /etc/apt/sources.list
echo "deb https://registry.example.com/debian-security trixie-security main" >> /etc/apt/sources.list
echo 'Acquire::http::Proxy "false";' > /etc/apt/apt.conf.d/99no-proxy
echo 'Acquire::https::Proxy "false";' >> /etc/apt/apt.conf.d/99no-proxy
echo 'APT::Install-Recommends "false";' > /etc/apt/apt.conf.d/99no-recommends
EOF
)

make docker-image ARCH=amd64 PUSH=true \
    APT_SETUP_CMD="$APT_SETUP_CMD" \
    CA_BUNDLE=/path/to/your-corp-ca-bundle.crt \
    CARGO_REGISTRY_MIRROR="sparse+https://registry.example.com/api/cargo/rust-remote/index/" \
    RUST_BUILD_IMAGE=registry.example.com/rust:1-bookworm \
    BASE_IMAGE=registry.example.com/distroless/cc-debian13:nonroot \
    REGISTRY=registry.example.com \
    ORG=my-org \
    IMAGE_TAG=v0.1.0
```

If a network runs a TLS-intercepting proxy (or an internal-only registry with
a private CA), the public `RUST_BUILD_IMAGE` won't already trust it — apt
fails with a misleading `does not have a Release file` and cargo fails with
`SSL routines::unexpected eof while reading`, even though the mirror URL
itself is correct. `CA_BUNDLE` fixes both at once: point it at the CA
cert/bundle file, and the Makefile mounts it into the container and runs
`update-ca-certificates` before apt or cargo touch the network. There's no
need to set `Acquire::https::CaInfo` yourself once `CA_BUNDLE` is set — the
cert becomes part of the container's system trust store.

Debian 12+ (bookworm, trixie) images ship their default sources in the newer
deb822 format at `/etc/apt/sources.list.d/*.sources`, separate from the
legacy `/etc/apt/sources.list`. apt merges *both* — so if your `APT_SETUP_CMD`
only overwrites the legacy file (as the example above does), the untouched
deb822 default still points at `deb.debian.org` and apt keeps trying to reach
it alongside your mirror. The `rm -f /etc/apt/sources.list.d/*.sources` line
in the example removes that default first.

If apt succeeds against the mirror but `cargo build` still fails with `SSL
routines::unexpected eof while reading` even with `CA_BUNDLE` set correctly,
the cause is usually not certificate trust at all: `cargo` links a statically
vendored libcurl/OpenSSL that can be a different (often newer) minor version
than the system's. OpenSSL 3.6+ defaults to advertising a post-quantum hybrid
TLS 1.3 key-exchange group (`X25519MLKEM768`) in its `ClientHello`, which some
TLS-inspection appliances can't parse and silently reset the connection on —
while `apt`/`curl` (linked against an older system OpenSSL without that
group) sail through the identical host untouched. Whenever
`CARGO_REGISTRY_MIRROR` is set, the Makefile passes
`cargo --config 'http.ssl-version="tlsv1.2"'`, which sidesteps TLS 1.3 group
negotiation entirely and is the reliable fix for this symptom.

Source replacement and the TLS pin are passed to `cargo` via `--config` CLI
flags rather than the `CARGO_SOURCE_*` env vars: mixing `CARGO_SOURCE_*` env
vars with any `--config` flag was observed to silently break the config
merge (the env-derived source config would apply, but `http.ssl-version`
would then be ignored) — set both together the same way to avoid this.
`CARGO_HTTP_CAINFO` and `CARGO_HTTP_MULTIPLEXING` are unaffected and stay as
env vars.

`APT_SETUP_CMD` and `CARGO_REGISTRY_MIRROR` are passed to the build container
via environment passthrough (`export` + `docker run -e VARNAME`), not Make
text substitution, so quotes and shell operators (`&&`, `>`) in the value
reach the container unmodified.

### Testing changes locally

`cargo test`/`cargo clippy`/`cargo fmt` can't run natively on macOS
(`tss-esapi-sys`'s build script has no `aarch64-darwin` bindings — it only
knows how to link against a real Linux `libtss2-dev`). On Apple Silicon in
particular, `--platform linux/amd64` runs under QEMU emulation unless
[Rosetta emulation is enabled in Docker Desktop](https://docs.docker.com/desktop/settings-and-maintenance/settings/#general)
(Settings → General → "Use Rosetta for x86_64/amd64 emulation") — without
it, expect `collect2`/GCC to segfault partway through linking, a real QEMU
bug, not a code problem. **The Rosetta option needs Docker Desktop 4.29.0+**
(released early 2024) — on an older install the setting simply isn't in the
UI at all (no error, no hint that it's version-gated), which reads as "this
Mac doesn't support it" when the real fix is just updating Docker Desktop.
Check via `docker version --format '{{.Server.Platform.Name}}'` (or Docker
Desktop → About) if the checkbox seems to be missing. On Linux, everything
below can run directly on the host with plain `cargo fmt`/`cargo
clippy`/`cargo test` instead of through a container.

Run the full quality gate the same way CI/`make docker-image` builds the
binary — inside `RUST_BUILD_IMAGE`, with the mirror/CA settings from the
table above (omit `CARGO_REGISTRY_MIRROR`/`CA_BUNDLE`/`APT_SETUP_CMD`
entirely on an open network):

```sh
docker run --rm --platform linux/amd64 \
  -v "$(pwd)":/src -w /src \
  -v sceau-cargo-registry:/usr/local/cargo/registry \
  -v sceau-cargo-git:/usr/local/cargo/git \
  -v sceau-target-amd64:/build-target \
  -v "$CA_BUNDLE":/usr/local/share/ca-certificates/build-ca-bundle.crt:ro \
  -e CARGO_TARGET_DIR=/build-target \
  -e CARGO_REGISTRY_MIRROR \
  "${RUST_BUILD_IMAGE:-rust:1-trixie}" bash -c '
    set -euo pipefail
    export DEBIAN_FRONTEND=noninteractive
    update-ca-certificates >/dev/null
    export CARGO_HTTP_CAINFO=/usr/local/share/ca-certificates/build-ca-bundle.crt
    export CARGO_HTTP_MULTIPLEXING=false
    apt-get update -qq && apt-get install -y -qq libtss2-dev protobuf-compiler binutils >/dev/null
    rustup component add rustfmt clippy >/dev/null 2>&1
    export RUSTFLAGS="-C link-arg=-fuse-ld=bfd"   # see "rust-lld under QEMU" below
    cargo fmt --all -- --check
    cargo clippy --all-targets --all-features \
      --config "source.crates-io.replace-with=\"mirror\"" \
      --config "source.mirror.registry=\"$CARGO_REGISTRY_MIRROR\"" \
      --config "http.ssl-version=\"tlsv1.2\"" -- -D warnings
    cargo test \
      --config "source.crates-io.replace-with=\"mirror\"" \
      --config "source.mirror.registry=\"$CARGO_REGISTRY_MIRROR\"" \
      --config "http.ssl-version=\"tlsv1.2\""
  '
```

Notes on the pieces above (each was a real failure found live, not a
preemptive precaution):

- The three named volumes (`sceau-cargo-registry`, `sceau-cargo-git`,
  `sceau-target-amd64`) persist across runs, so a second invocation only
  recompiles what actually changed — without them, every run recompiles the
  entire dependency tree from scratch. `CARGO_TARGET_DIR=/build-target`
  (a path outside the bind-mounted `/src`) also keeps the container's
  `linux/amd64` build artifacts from colliding with your host's own
  `target/` directory if you also run `cargo check` natively — mixing the
  two in one directory invalidates every fingerprint on every run.
- **`rust-lld` under QEMU**: rustc's bundled `lld` linker segfaults
  intermittently under `qemu-user` emulation (`collect2`/GCC crashing
  mid-link, not `rustc` itself). `binutils` + `RUSTFLAGS="-C
  link-arg=-fuse-ld=bfd"` forces the classic BFD linker, which doesn't hit
  the QEMU incompatibility. Enabling Rosetta (see above) makes this
  unnecessary, but the flag is harmless either way.
- `rustfmt`/`clippy` components aren't part of the base `rust:1-trixie`
  image and don't persist in the named cache volumes (they install into the
  toolchain directory, not `/usr/local/cargo`) — `rustup component add`
  re-runs (fast, cached by apt/rustup's own state) on every invocation.
- `sceau` gained a `src/lib.rs` (2026-09-06, alongside `src/main.rs`) so
  `tests/` integration tests can reach modules like `fleet` directly — it
  was binary-only before that. `cargo test --lib <name-filter>` now runs a
  subset from the library target; plain `cargo test <name-filter>` still
  works too and also picks up the (currently empty) binary target.

### Deploying to a test node

```sh
IMAGE_REF=registry.example.com/my-org/sceau:v0.1.0 \
  HOSTS="user@node1 user@node2" \
  make deploy-test
```

`deploy-test` copies a built `sceau` binary + its TPM2-TSS shared libs +
its systemd unit (`contrib/systemd/sceau.service`, if present) onto one or
more test nodes over SSH — for iterating before this is packaged as a
Kairos sysext, not the eventual provisioning path. On Linux it reuses
`make build-linux-$ARCH`'s local staging directly; on macOS (which can't
produce a Linux binary itself) it pulls and extracts `IMAGE_REF` instead, so
`IMAGE_REF` is required there. The unit is installed to
`/etc/systemd/system/` and `daemon-reload`ed on each host, but never
enabled or started — `deploy-test` prints the `systemctl enable --now`
command to run once you're ready.

If you're redeploying the *same* tag repeatedly while iterating (e.g.
`IMAGE_TAG=v0.1.0` on every push instead of a fresh tag each time),
`docker-image` runs `crane delete $(IMAGE_REF)` before every push — some
registries (observed against an Artifactory Docker repo) report a
successful push while silently leaving an existing tag's content
unchanged, so the delete-then-push is unconditional rather than relying on
the registry to overwrite in place. Requires `crane` (go-containerregistry)
on `PATH`; it reuses whatever credential helper `docker login` already
configured, no separate auth setup needed.

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
ExecStart=/usr/local/bin/sceau serve --socket /run/sceau/sceau.sock
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

## Supply chain

Every pushed image and released binary carries verifiable evidence of what it
is and where it came from (ADR-0002, [ADR-0006](docs/adr/0006-vex-slsa-and-attestation-parity.md)):

| Evidence | Produced by | Verify with |
| --- | --- | --- |
| Cosign signature (keyless, **by digest**) | `docker` job | `cosign verify ghcr.io/firestoned/sceau@<digest> --certificate-identity-regexp '.*' --certificate-oidc-issuer https://token.actions.githubusercontent.com` |
| Build provenance (image + each binary tarball) | `attest`, `sign-artifacts` | `gh attestation verify <artifact> --repo firestoned/sceau` |
| SLSA Build L3 provenance | `slsa-provenance` | `slsa-verifier verify-artifact <tarball> --provenance-path sceau-<version>.intoto.jsonl --source-uri github.com/firestoned/sceau` |
| CycloneDX SBOM (binary + image) | `build`, `docker` | any CycloneDX tool |
| OpenVEX triage document | `build-vex` | `cosign verify-attestation --type openvex ghcr.io/firestoned/sceau@<digest>` |

Images are signed by digest, never by tag — a tag is mutable, so a signature
bound to one says nothing durable about the bytes you pulled.

### Vulnerability triage

The container scan is **Grype** (pinned `GRYPE_VERSION`, ≥ 0.118.0), run with
`--vex` so justified findings never reach the Code Scanning tab. Two sources
feed the VEX document:

- **Automatic** — `auto-vex-presence` (`crates/sceau-vex`) emits
  `component_not_present` for any finding whose package URL appears in no
  image SBOM. Nothing to maintain by hand.
- **Curated** — `.vex/*.json`, one OpenVEX document per advisory, for findings
  where the component *is* present but the vulnerable path is unreachable.
  That is most of what sceau carries: the TPM TSS shared libraries and their
  glibc dependencies are genuinely in the image, so only a human can justify
  those.

```sh
make vex-validate      # every .vex/*.json parses and merges
make vex-assemble      # merge curated statements to stdout
make grype-triage SCAN_IMAGE_REF=ghcr.io/firestoned/sceau@<digest>
make vex-auto-presence # derive presence statements from the triage scan
```

> **Do not downgrade Grype below 0.118.0.** Older releases accept `--vex` and
> then emit the suppressed findings anyway — the pipeline looks like it works
> while suppressing nothing. See ADR-0006.

To dismiss a new finding, write `.vex/<ADVISORY>.json` (see
[`.vex/README.md`](.vex/README.md)) — never an ignore-list in workflow YAML.

## Status

Early skeleton: KMS v2 gRPC server + TPM seal/unseal are implemented; PCR
policy binding, graceful SRK eviction under memory pressure, and e2e tests
against a swtpm are next.
