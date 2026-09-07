// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0
//! Unit tests for `fleet.rs`.
//!
//! Every function in `fleet.rs` now requires a live `Context`/TPM —
//! `duplicable_storage_public` used to be the one exception (pure in-memory
//! `Public`/attribute construction), but the 2026-09-06 `TPM2_Duplicate`
//! fix gave it a `context: &mut Context` parameter too (it now runs a real
//! `Trial` session to compute the object's `authPolicy`). Per
//! `rules/testing.md`, TPM-dependent tests belong in `swtpm`-gated
//! integration tests under `tests/`, not here — none exist yet (tracked in
//! the ADR-0003 roadmap's Phase 2 checklist).

#[cfg(test)]
mod tests {}
