<!--
Copyright (c) 2026 Erick Bourgeois, sceau
SPDX-License-Identifier: Apache-2.0
-->
# 0006 — OpenVEX, SLSA provenance, and build attestation parity with banlieue

- **Status:** Accepted
- **Date:** 2026-09-07
- **Deciders:** Erick Bourgeois
- **Related:** Extends **ADR-0002** (release and supply-chain pipeline) —
  specifically, it resolves three of the four items ADR-0002 listed under
  *Alternatives considered / deferred*. Mirrors banlieue's
  `docs/adr/0006-release-and-supply-chain-pipeline.md`, which is the reference
  implementation this ADR ports.

## Why this number

ADR-0004 is provisionally reserved for the `k0smotron`/CAPI multi-tenant
fleet-key topic (see ADR-0003's roadmap, Phase 10) and ADR-0005 for
attestation-gated cluster join (see
`.github/community/adr-0005-attestation-gated-cluster-join-roadmap.md`).
Neither is written yet, and neither is related to this decision, so this ADR
takes **0006** rather than consuming a reserved number. It also happens to
align with banlieue's own ADR-0006, which is the document being ported.

## Context

ADR-0002 established sceau's pipeline in a deliberately reduced form: one
distroless image, Cosign signature, CycloneDX SBOM, Trivy scan (advisory,
`exit-code: 0`). It explicitly deferred SLSA provenance, OpenVEX, and the
Chainguard variant, on the reasoning that triage volume did not yet justify
them and no release had been cut.

Three things have changed that reasoning:

1. **Trivy-as-advisory produces no durable triage record.** A finding that is
   genuinely not exploitable against sceau — a CVE in a base-image package
   nothing links, say — is re-reported on every run with no machine-readable
   statement of *why* it was dismissed. Consumers see the same noise and have
   no way to distinguish "not triaged" from "triaged and not applicable."
2. **sceau's attack surface is unusually base-image-heavy for a Rust binary.**
   Unlike banlieue (pure Rust, rustls, no FFI), sceau links dynamically
   against `libtss2-esys` and friends, and the Makefile stages those shared
   objects plus their transitive glibc dependencies into the image rootfs.
   That is exactly the population of packages that generates recurring CVEs
   requiring per-CVE justification rather than a version bump.
3. **The pipeline is the security boundary's evidence.** sceau touches every
   DEK the cluster issues. "Verify the artifact is what the source produced"
   needs provenance a verifier can check (`slsa-verifier`, `gh attestation
   verify`), not just a signature proving *someone* signed *something*.

The governing constraint is that banlieue is the reference implementation and
sceau should not invent a second, divergent supply-chain shape. Anything
sceau does differently should be a deliberate, recorded difference.

## Decision

**Bring sceau's release pipeline to feature parity with banlieue's, with
three recorded deviations.**

### 1. Grype + OpenVEX replaces advisory-only Trivy

- The container scan becomes **Grype** (pinned, `GRYPE_VERSION`), producing
  SARIF uploaded to GitHub Code Scanning, run with `--vex` against an
  assembled OpenVEX document so justified findings are suppressed at the
  source rather than mentally filtered by a human reading the tab.
- **Grype is pinned at ≥ 0.118.0.** Older releases (verified in banlieue
  against 0.87.0) accept `--vex` and then silently emit the suppressed
  findings anyway. A VEX pipeline on top of a Grype that ignores VEX is worse
  than no VEX pipeline, because it looks like it works. Do not downgrade.
- Curated, hand-authored statements live in **`.vex/*.json`**, one OpenVEX
  document per CVE, merged by `vexctl` (pinned, `VEXCTL_VERSION`).
- The assembled document is **Cosign-attested to the image digest**
  (`cosign attest --type openvex`), so the justification travels with the
  artifact instead of living only in the repo.
- Trivy is dropped rather than kept alongside Grype: two scanners with two
  finding vocabularies means two triage backlogs, and only one of them would
  have VEX suppression wired up.

### 2. Presence-based auto-VEX, in a new `crates/sceau-vex` crate

A new workspace member ships one binary, **`auto-vex-presence`**, ported from
`banlieue-vex`. It reads a raw (no-VEX) Grype report plus the image SBOMs and
emits `not_affected` / `component_not_present` for every finding whose
affected purl appears in no SBOM, skipping any CVE already covered by a
curated `.vex/*.json`.

`component_not_present` is the one OpenVEX justification with a purely
mechanical definition — the SBOM *is* the definition of what ships — so it is
the only one safe to derive automatically. Everything else stays
human-authored.

Two fail-closed properties are carried over from banlieue verbatim, because
both are the difference between a safe tool and a dangerous one:

- **An SBOM that parses but has zero components is an error, not a signal.**
  Treating it as "nothing is present" would emit `not_affected` for *every*
  finding, silently, from a truncated download.
- **All file input is size-capped** before being slurped, so a hostile or
  runaway artifact fails fast instead of exhausting the runner.

**Deviation from banlieue:** `auto-vex-reachability` (symbol-table–based
suppression) is **not** ported in this ADR. It is the higher-risk of the two —
it reasons about whether vulnerable code is *reachable*, which is a claim
about program behavior rather than about set membership — and it depends on a
curated CVE→symbol map that sceau has no entries for yet. Adding it later is
additive: the crate, the `.vex/` layout, and the CI job graph are all shaped
to accept a second auto-VEX producer without restructuring.

**Deviation from banlieue:** the crate uses the `time` crate for its RFC-3339
timestamp default rather than adding `chrono`, since `time` is already in
sceau's dependency graph. No new dependency family enters the tree.

### 3. SLSA Build L3 provenance and GitHub build attestations

- Release binaries are tarballed per architecture, **Cosign-signed** and
  **`actions/attest-build-provenance`-attested**.
- The official **`slsa-framework/slsa-github-generator`** reusable workflow
  (pinned to a release *tag* — `slsa-verifier` rejects non-released refs)
  generates a Build L3 `.intoto.jsonl` over the tarball hashes, attached to
  the release.
- Each pushed **image digest** additionally gets an
  `actions/attest-build-provenance` attestation pushed to the registry.

### 4. arm64 joins the matrix; images are signed by digest

- The build job becomes a matrix over `ubuntu-24.04` and `ubuntu-24.04-arm`,
  producing native binaries plus their staged TSS rootfs per architecture.
  Kairos/k0s on arm64 is a real deployment target for a TPM plugin, and SLSA
  provenance over an amd64-only tarball would misrepresent what is shipped.
- The image becomes a genuine multi-arch manifest built from both
  pre-staged binary trees (the Dockerfile already keys off `TARGETARCH`), and
  Cosign signs **by digest** rather than by tag. A tag is mutable; a signature
  over a tag is a signature over whatever that tag points at *now*.

**Deviation from banlieue:** sceau keeps **one image variant** (distroless).
The CI jobs are nevertheless written as a variant matrix with a single entry,
so adding Chainguard later is a matrix-entry change rather than a rewrite.
The reason for not adding it now is specific to sceau: the image must carry
`libtss2` shared objects staged from a Debian build environment, and a
wolfi-based variant needs that staging redone against apk-sourced libraries —
real work, not a second `FROM` line.

## Consequences

**Positive**

- Every dismissed CVE has a machine-readable, attested justification, and the
  Code Scanning tab shows only findings that are actually open.
- Consumers can verify provenance end-to-end: `cosign verify` (signature),
  `cosign verify-attestation --type openvex` (triage), `gh attestation
  verify` (build provenance), `slsa-verifier` (SLSA L3).
- The mechanical share of triage — "is this package even in the image?" —
  stops reaching a human at all.
- arm64 consumers get first-class, signed, attested artifacts.

**Negative / costs**

- The CI graph roughly doubles in job count and gains a fan-in
  (`grype-triage` → `auto-vex-presence` → `build-vex` → `grype`). The
  triage scan is deliberately a separate job from the final scan so the
  graph stays acyclic — the alternative is a scan that consumes a VEX
  document derived from its own output.
- A workspace conversion: the repo root gains a `[workspace]` section and
  `make test` / `make lint` become `--workspace`-scoped.
- Auto-derived suppressions are only as trustworthy as the SBOM. The
  zero-component guard is what keeps a bad SBOM from becoming a blanket
  dismissal, and it must not be relaxed.
- Grype's VEX handling is version-sensitive in a silent way (see above). The
  pin is load-bearing and needs a comment wherever it appears.
- VEX/SLSA/attestation jobs are all `github.event_name != 'pull_request'`, so
  fork PRs never need elevated tokens — but it also means the VEX pipeline is
  not exercised on PRs. A PR that breaks `.vex/` parsing surfaces on merge to
  main, not before. `make vex-validate` exists to catch that locally.

## What this rules out

- A second container scanner. Grype + VEX is the scan; findings are triaged
  by writing a `.vex/*.json`, not by adding an ignore-list to another tool.
- Signing by tag anywhere in the pipeline.
- Hand-maintained "known issues" lists in README or workflow YAML — the
  `.vex/` directory is the only place a dismissal is recorded.
