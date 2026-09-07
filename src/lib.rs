// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0
//! `sceau` as a library — exists so `tests/` integration tests can reach
//! modules like `fleet` directly, instead of only ever running through the
//! `sceau` binary. `main.rs` is a thin binary on top of this crate.

pub mod authz;
pub mod certs;
pub mod cli;
pub mod enroll;
pub mod fleet;
pub mod join;
pub mod kms;
pub mod tpm;
