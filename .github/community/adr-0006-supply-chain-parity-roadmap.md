<!-- Copyright (c) 2026 Erick Bourgeois, sceau -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# sceau ADR-0006 — Supply-Chain Parity (OpenVEX / SLSA / Attestation) — Roadmap

**Author:** Erick Bourgeois
**Source decision:** `docs/adr/0006-vex-slsa-and-attestation-parity.md`
(extends ADR-0002; resolves three of its four deferred items).
**Repo(s) in scope:** `firestoned/sceau` only. The reference implementation
being ported lives in `firestoned/banlieue`
(`docs/adr/0006-release-and-supply-chain-pipeline.md`, `crates/banlieue-vex`),
but no change to that repo is required or planned here — this is a one-way
port. Stated explicitly per this project's roadmap convention rather than
assumed.

> Status legend: ☐ not started · ◐ in progress · ☑ done.
> Effort: S ≤ half day · M ≤ 2 days · L > 2 days.

## Ordering rationale

The phases below are sequenced by what each one *unblocks*, not by size:

1. **The scanner swap has to carry the existing triage with it.** sceau had 20
   justified suppressions in `.trivyignore` as of 2026-09-07. Replacing Trivy
   with Grype without migrating those first would reopen all 20 under a new
   scanner and make the parity work look like a regression. Phase 1 therefore
   does the migration, and only then is Trivy removed.
2. **VEX tooling before VEX consumption.** The `grype --vex` job is only
   meaningful once there is a document to feed it, which needs both the
   curated directory and the auto-derivation binary.
3. **Provenance last, and gated on a live run.** Signing, attestation and SLSA
   all depend on OIDC tokens that exist only in a real GitHub Actions run.
   Nothing about them can be verified locally, so they are the last thing
   written and the first thing to check on the first push to `main`.

The single hard gate is at the end of Phase 4: **no phase-5 item may be
called done on the strength of the YAML being written.** This project's own
history is the reason — the ADR-0003 TPM work needed real-hardware-driven
fixes that no amount of documentation research caught in advance, and
banlieue's supply-chain work hit two live-only failures on 2026-09-07 alone
(Grype silently ignoring `--vex` below 0.118.0; the SLSA generator's stale
repo-visibility check). Both are already worked around here on the strength
of *banlieue's* live evidence, not sceau's — which is exactly why sceau's own
first run still has to be watched.

## Phase 1 — Retire Trivy without losing triage ☑ done

- [x] Migrate all 20 `.trivyignore` entries to `.vex/*.json`, one OpenVEX
      document per advisory, preserving the original justification (effort: M)
- [x] Add a "drop this statement when…" condition to each migrated entry —
      the original ignore-list had per-group notes but no per-CVE exit
      criterion, so nothing said when a suppression should expire (effort: S)
- [x] Confirm all 20 parse and merge (`make vex-validate`,
      `make vex-assemble-all` → 20 statements) (effort: S)
- [x] Delete `.trivyignore` and the Trivy job (effort: S)

> **Gap called out while writing this roadmap, not carried over from prior
> discussion:** the 20 migrated statements are all `libc6`/`zlib1g` findings
> whose packages *are* in the image SBOM. `auto-vex-presence` will never
> derive them — they are permanently hand-maintained. Nothing re-checks
> whether a fixed package version has since shipped, so they will silently
> outlive their justification. See Phase 6.

## Phase 2 — Auto-VEX tooling ☑ done

- [x] Convert the repo root to a workspace root; add `crates/sceau-vex`
      (effort: S)
- [x] Port `auto-vex-presence` tests first (29 cases: happy path, purl in
      SBOM, already-triaged, missing purl, multi-SBOM union, de-dup,
      determinism, malformed inputs, both fail-closed guards) (effort: M)
- [x] Implement the module against those tests; substitute `time` for
      `chrono` in the CLI so no new dependency family enters the tree
      (effort: S)
- [x] `make sbom` discards `crates/*/*.cdx.json` so the released SBOM
      describes only the shipped binary (effort: S)
- [x] Local end-to-end smoke test: a purl in the SBOM is skipped, one absent
      from it is emitted (effort: S)

## Phase 3 — Makefile targets ☑ done

- [x] `vexctl-install`, `grype-install` (both version-pinned) (effort: S)
- [x] `grype-triage` (raw, no VEX) and `grype-scan` (`--vex`, SARIF) (effort: S)
- [x] `vex-validate`, `vex-assemble`, `vex-assemble-all`, `vex-auto-presence`
      (effort: M)
- [x] `docker-image-prestaged` — multi-arch buildx from already-staged
      `binaries/<arch>/`, with `--metadata-file` — plus `docker-digest`
      (effort: M)
- [x] Every target CI calls dry-runs cleanly (`make -n`) (effort: S)

## Phase 4 — Workflow rework ☑ done (written, not yet run)

- [x] `build` becomes an amd64 + arm64 matrix on native runners (effort: M)
- [x] Split `test` out of `build` so packaging jobs can depend on it directly
      (effort: S)
- [x] `docker` builds a real multi-arch manifest from both staged trees;
      signs **by digest**; emits an image SBOM (effort: M)
- [x] `attest`, `grype-triage`, `auto-vex-presence`, `build-vex`, `grype`
      jobs, with the acyclic triage→derive→assemble→scan fan-in (effort: L)
- [x] `sign-artifacts` (tarball + Cosign + build provenance), per arch. The
      tarball carries `rootfs/` as well as the binary — sceau links libtss2
      dynamically, so a bare binary is not a runnable artifact (effort: M)
- [x] `generate-provenance-subjects` + `slsa-provenance` (generator pinned to
      a release tag) (effort: M)
- [x] `vex-validate` job — the only VEX gate that runs on PRs, since the
      assemble/attest chain needs a pushed image (effort: S)
- [x] Static checks pass: YAML parses, job graph is acyclic, every
      `needs.X.outputs.Y` is declared, every action SHA-pinned (effort: S)

**Gate:** everything below this line is unverified. The jobs need OIDC
tokens, a pushed image, and a GHCR digest — none of which exist outside a
real Actions run. Do not mark Phase 5 items done from a green YAML lint.

## Phase 5 — Live verification on the first push to `main` ☐ not started

- [ ] `build` succeeds on `ubuntu-24.04-arm`. The arm64 leg has never run:
      `make build-linux-arm64` takes the host-toolchain path on a native
      runner, which is a different code path from the container fallback that
      local macOS builds use (effort: S)
- [ ] The multi-arch manifest actually contains both platforms
      (`docker buildx imagetools inspect`) (effort: S)
- [ ] `make docker-digest` returns the pushed manifest-list digest — confirm
      buildx writes `containerimage.digest` for a multi-platform `--push`,
      and that Cosign/attest/Grype all resolve that same digest (effort: S)
- [ ] `cosign verify` succeeds against the digest (effort: S)
- [ ] `gh attestation verify` succeeds for the image and both tarballs
      (effort: S)
- [ ] `cosign verify-attestation --type openvex` returns the assembled
      document (effort: S)
- [ ] Grype's SARIF shows the 20 migrated CVEs **suppressed** — this is the
      real test of the whole phase-1 migration, and the failure mode is
      silent (a Grype that ignores `--vex` produces a *plausible* report)
      (effort: M)
- [ ] `slsa-verifier verify-artifact` validates the generated
      `.intoto.jsonl` (effort: M)
- [ ] Confirm the `private-repository: true` workaround is still needed;
      drop it if the generator's visibility check has been fixed (effort: S)

## Phase 6 — Triage hygiene ☐ not started

Surfaced while writing this roadmap; not part of the ADR-0006 decision.

- [ ] Something that re-checks whether a curated `.vex/` statement has been
      outlived by a fixed package version. Twenty permanently-hand-maintained
      statements with no expiry check is how a suppression list rots into a
      blanket dismissal (effort: M)
- [ ] Decide whether an unsuppressed high/critical should fail the build.
      Today `grype-scan` is report-only — the SARIF lands in Code Scanning but
      nothing blocks (effort: S)

## Phase 7 — Deferred parity items ☐ not started

Recorded so the deviations from banlieue stay visible rather than becoming
accidental permanent state.

- [ ] `auto-vex-reachability` + `.vex/.affected-functions.json`. The crate,
      the `.vex/` layout and the CI fan-in are already shaped to accept a
      second auto-VEX producer without restructuring. Note this would
      partially automate the Phase-1 migrated statements, which are exactly
      the "symbols not imported" claims the tool exists to check (effort: L)
- [ ] Chainguard image variant. The CI jobs are already variant-matrices with
      one entry; the real work is restaging libtss2 from apk rather than
      Debian packages, not a second `FROM` line (effort: L)
