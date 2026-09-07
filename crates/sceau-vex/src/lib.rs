// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0
//! # sceau-vex
//!
//! Auto-VEX derivation tooling for the sceau release/supply-chain pipeline
//! (see `docs/adr/0006-vex-slsa-and-attestation-parity.md`). Ported from
//! banlieue's `banlieue-vex` crate, which is this project's reference
//! implementation for supply-chain work.
//!
//! One pure module drives one thin CLI binary:
//!
//! - [`auto_vex_presence`] — emit `not_affected + component_not_present` for
//!   Grype findings whose affected purl is absent from every image SBOM.
//!
//! banlieue additionally ships `auto-vex-reachability` (symbol-table-based
//! suppression). ADR-0006 deliberately does not port it yet: it reasons about
//! whether vulnerable code is reachable rather than about set membership, and
//! sceau has no curated CVE→symbol entries. This crate is laid out so adding
//! it later is additive.
//!
//! This crate is CI-only tooling and is never linked into the `sceau` binary.
//! All I/O lives in `src/bin/`; the module itself is pure and exhaustively
//! tested.

pub mod auto_vex_presence;
