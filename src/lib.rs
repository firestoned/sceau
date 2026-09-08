// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0
//! `sceau` as a library — exists so `tests/` integration tests can reach
//! modules like `fleet` directly, instead of only ever running through the
//! `sceau` binary, and so external crates can link the pure parsers without
//! a TPM. The `fuzz/` cargo-fuzz workspace depends on the latter: it drives
//! [`tpm::envelope_decode`] and the KMS protobuf decoders directly
//! (ADR-0003, fuzzing). `main.rs` is a thin binary on top of this crate.

pub mod authz;
pub mod certs;
pub mod cli;
pub mod enroll;
pub mod fleet;
pub mod join;
pub mod kms;
pub mod tpm;
