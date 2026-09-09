<!-- Copyright (c) 2026 Erick Bourgeois, sceau -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Roadmaps

Unlike some sibling projects (e.g. `banlieue`, whose roadmaps live outside
that repo entirely), `sceau`'s roadmaps are tracked **in this repository**,
under [`.github/community/`](.github/community/). This file is the index —
add a line here whenever a new roadmap doc is added or an existing one's
status changes materially.

Each roadmap tracks the implementation status of one ADR (or a related
group of ADRs) as an ordered, checkbox-driven phase list — what's done,
what's in progress, what's not started, and why the ordering is what it is.
See `.claude/rules/roadmaps.md` for the format and conventions every
roadmap doc follows.

| Roadmap | Tracks | Status |
| --- | --- | --- |
| [ADR-0003 — Fleet key duplication for HA multi-controller](.github/community/adr-0003-fleet-key-ha-roadmap.md) | `docs/adr/0003-fleet-key-duplication-for-ha-multi-controller.md` | Phases 2-5 done, live-verified on real hardware — including the actual cross-node Secret decrypt this ADR exists for, and the enroll/join network+authz round-trip (both positive and negative paths); Phase 6 (EK hardening) in progress — operator allowlist + transport-key validation landed 2026-09-09; credential-activation attestation still open |
| [ADR-0005 (tentative) — Attestation-gated cluster join](.github/community/adr-0005-attestation-gated-cluster-join-roadmap.md) | not yet written (this roadmap's Phase 0) | Not started — blocked on ADR-0003 Phase 6 landing first; reuses its attestation primitives to gate k0s join-token issuance itself, not just fleet-key duplication |
| [ADR-0006 — Supply-chain parity (OpenVEX / SLSA / attestation)](.github/community/adr-0006-supply-chain-parity-roadmap.md) | `docs/adr/0006-vex-slsa-and-attestation-parity.md` | Phases 1-4 done (Trivy suppressions migrated to `.vex/`, `crates/sceau-vex` + tests, Makefile targets, workflow rewritten); Phase 5 — live verification on the first push to `main` — not started and explicitly gating: signing, attestation and SLSA cannot be exercised outside a real Actions run |
