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

## Alternatives considered / deferred

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
