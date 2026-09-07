<!--
Copyright (c) 2026 Erick Bourgeois, sceau
SPDX-License-Identifier: Apache-2.0
-->
# 0002 — Release and supply-chain pipeline

- **Status:** Accepted
- **Date:** 2026-09-04
- **Deciders:** Erick Bourgeois
- **Related:** ADR-0001 (the plugin being shipped); `rules/github-workflows.md` (Makefile-driven, `firestoned/github-actions` composites). Modeled on banlieue's `docs/adr/0006-release-and-supply-chain-pipeline.md`.

## Context

sceau ships a deployable artifact: a single Linux binary that must land on
Kairos hosts (via image bundling or a container). Because the plugin is a
security boundary — it touches every DEK the cluster issues — consumers must
be able to verify that a released binary/image is exactly what the source
produced. The reference pipeline is banlieue (ADR-0006 there), which sceau
adopts in a reduced form appropriate to a single-binary repo.

## Decision

**The `sceau` binary and its distroless container image are the released
artifacts, and every release carries the core supply-chain attestation set.**

1. **Binary** — built on Linux runners (`libtss2-dev` + `protobuf-compiler`
   installed in the workflow; build logic in the Makefile), attached to the
   GitHub Release as a tarball with a CycloneDX SBOM (`make sbom`,
   cargo-cyclonedx).
2. **Container image** — one variant: distroless
   (`gcr.io/distroless/cc-debian13:nonroot`, digest-pinned) built from the
   pre-built Linux binary plus the TPM TSS runtime shared libraries staged by
   the Makefile (`make docker-image`). Never `cargo build` inside the image.
   Pushed to `ghcr.io/firestoned/sceau`, **Cosign-signed** (keyless/OIDC) by
   digest, and scanned with **Trivy** (SARIF to Code Scanning).
3. **SBOM** — CycloneDX for the binary, attached on release.
4. **SLSA provenance** — deferred (see below).

**Conventions retained from the repo rules:**
- Workflows use `firestoned/github-actions/*` composite actions where one
  exists; third-party actions are SHA-pinned with version comments.
- Build/test/lint/audit/SBOM logic lives in the Makefile; workflows install
  tools and call `make` targets.
- Top-level `GITHUB_TOKEN` is read-only; jobs that push images
  (`packages: write`), sign (`id-token: write`), or upload SARIF
  (`security-events: write`) declare it at the job level.

## Consequences

**Positive**
- Every pushed image is signed and scanned; every release has an SBOM —
  consumable by `cosign verify` and standard SBOM tooling.
- A single distroless variant keeps the pipeline small enough to maintain.

**Negative / costs**
- The distroless image needs the TSS runtime libraries staged alongside the
  binary (the base image has no libtss2); the Makefile owns that staging.
- No VEX/triage automation (banlieue's auto-vex machinery is not ported);
  Trivy findings are triaged by hand for now.

## Addendum (2026-09-04) — local build strategy: host toolchain first, container fallback

**Context:** `make build-linux-*` originally always shelled out to `docker run
rust:1-bookworm ...` to get a Linux build environment with `libtss2-dev`. On a
developer machine without unrestricted access to Docker Hub (e.g. behind a
corporate registry allowlist), this made local Linux builds impossible even
though a perfectly good host cross-toolchain was available. banlieue's
`build-linux-*` avoids this by cross-compiling natively on the host via a gcc
cross-toolchain, falling back to a container only when no cross-linker exists.

**Decision:** `build-linux-*` now tries the host-toolchain path first —
native compile (or a host-installed `x86_64-linux-gnu-gcc` /
`aarch64-linux-gnu-gcc` cross-linker) — and only falls back to the
`RUST_BUILD_IMAGE` container when `pkg-config` can't find `tss2-esys` linkable
for the target. Unlike banlieue's pure-Rust binary, sceau links against
libtss2 (a system C library via `tss-esapi`), so the host path only succeeds
when libtss2-dev is actually present for the target triple — in practice, that
means Linux CI runners (which `apt-get install libtss2-dev` before building)
take the native path, while cross-arch local builds and macOS dev machines
still need the container. `RUST_BUILD_IMAGE` remains a Makefile variable so it
can be pointed at a private registry mirror instead of Docker Hub.

**Consequences:**
- CI (Linux, native arch) no longer needs a container at all for
  `build-linux-*`.
- macOS/cross-arch local builds still require a container, and that
  container's registry is user-configurable (`RUST_BUILD_IMAGE=...`) to work
  around registries that block Docker Hub.
- No change to the released artifacts or supply-chain guarantees in the main
  Decision section above.
- The container fallback's own `apt-get install libtss2-dev` step needs
  `deb.debian.org` reachable, which networks that only allow a private
  registry mirror will also block. `APT_SETUP_CMD=<shell command>` runs
  verbatim inside the container before `apt-get update`. This is a raw hook
  rather than a templated hostname substitution: real private-mirror setups
  commonly need a custom repo path layout, a separate security repo, a
  corporate CA bundle, and proxy overrides all at once, and no single
  substitution pattern covers that combination. The value is passed through
  via `export` + `docker run -e APT_SETUP_CMD` (environment passthrough, not
  Make text substitution into the command line) so arbitrary shell content —
  embedded quotes, `&&`, redirects — reaches the container unmodified instead
  of being reinterpreted by the host shell.
- `cargo build` itself resolves against crates.io regardless of
  `RUST_BUILD_IMAGE`/`APT_SETUP_CMD`, and the same restricted networks block
  that too. `CARGO_REGISTRY_MIRROR=<sparse-url>` routes it to a private
  mirror, in both the host-toolchain and container code paths.
- A correct mirror URL is not sufficient on networks with a TLS-intercepting
  proxy or an internal-only registry backed by a private CA: the public
  `RUST_BUILD_IMAGE` doesn't already trust that CA, so apt fails with `does
  not have a Release file`. `CA_BUNDLE=<host path>` bind-mounts a CA
  cert/bundle into the container and runs `update-ca-certificates` before apt
  or cargo touch the network, adding it to the container's system trust
  store (and exporting `CARGO_HTTP_CAINFO` defensively).
- `SSL routines::unexpected eof while reading` from `cargo build` against a
  mirror, even with CA trust correctly configured and apt succeeding against
  the identical host, is a *distinct* failure mode confirmed by direct
  reproduction (not certificate trust, not HTTP/2 multiplexing — both were
  tried and ruled out): `cargo` statically vendors its own libcurl/OpenSSL,
  independent of the system's. OpenSSL 3.6+ defaults to advertising a
  post-quantum hybrid TLS 1.3 key-exchange group (`X25519MLKEM768`) in its
  `ClientHello`; forcing that group with `openssl s_client -groups
  X25519MLKEM768` against the same host reproduced an identical
  connection-reset, while classic groups (`X25519:P-256`) completed a normal
  handshake — confirming a TLS-inspection appliance in the path resets on the
  unrecognized/oversized `ClientHello` rather than on certificate content.
  `OPENSSL_CONF`-based group restriction has no effect (vendored builds
  disable runtime config auto-loading), so the fix is
  `cargo --config 'http.ssl-version="tlsv1.2"'`, which avoids TLS 1.3 group
  negotiation entirely. Whenever `CARGO_REGISTRY_MIRROR` is set, the Makefile
  passes this alongside the source-replacement config.
- Source replacement and `ssl-version` are both passed via `--config` CLI
  flags rather than `CARGO_SOURCE_*` env vars: reproduction showed that
  mixing `CARGO_SOURCE_*` env vars with *any* `--config` flag causes cargo to
  silently drop `http.ssl-version` (source replacement still worked, but the
  TLS pin did not) — a cargo config-merge quirk, not a Makefile bug. Setting
  both together via `--config` avoided it in every combination tested.
  `CARGO_HTTP_CAINFO` and `CARGO_HTTP_MULTIPLEXING` remain env vars, which
  did not exhibit this interaction.

## Alternatives considered / deferred

> **Update (2026-09-07):** the SLSA-provenance, OpenVEX/auto-vex, and Trivy
> deferrals below were resolved by **ADR-0006** (OpenVEX, SLSA provenance, and
> build attestation parity with banlieue), which also replaces the Trivy scan
> with Grype + `--vex`. The Chainguard-variant deferral still stands, with the
> reason restated there. This section is kept as written for the historical
> record.

- **SLSA provenance (Build L3 via slsa-github-generator).** Deferred: banlieue
  generates SLSA provenance for its release tarballs; sceau will adopt the
  same reusable workflow when the first real release is cut. The pipeline
  shape (per-job `id-token: write`, attestation-friendly artifacts) already
  accommodates it.
- **Chainguard image variant.** Deferred: banlieue ships both distroless and
  Chainguard; sceau starts with distroless only. A Chainguard glibc-dynamic
  variant (with `libtss2` packages via apk) is a likely future addition.
- **OpenVEX + auto-vex.** Deferred with the Trivy job kept advisory
  (`exit-code: 0`) until triage volume justifies it.
