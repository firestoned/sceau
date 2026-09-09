// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0
//! `sceau`'s CLI surface: five subcommands (2026-09-06 — replaced the
//! earlier flat `--genesis`/`--enroll`/`--join` flag design, which needed a
//! hand-validated `RawArgs`/`Mode`/`ModeError` layer just to reject
//! combinations clap's own `Subcommand` derive rejects structurally for
//! free, and let every mode see every other mode's flags whether relevant
//! or not).
//!
//! - `sceau serve` — today's ADR-0001 behavior: the steady-state KMS v2
//!   Unix-socket plugin.
//! - `sceau genesis [--force]` (ADR-0003) — create the fleet's duplicable
//!   sealing key. `--force` deletes and recreates it even if one already
//!   exists — needed for a key created before the `TPM2_Duplicate`
//!   `authPolicy` fix (2026-09-06, see `fleet.rs`'s module doc comment);
//!   such a key can never be duplicated.
//! - `sceau enroll --listen=<addr> [--max=<n>] [--timeout-secs=<d>]`
//!   (ADR-0003) — serve one bounded, authenticated `TPM2_Duplicate` to a
//!   joiner, then stop listening.
//! - `sceau join --seed=<addr>` (ADR-0003) — dial a seed and receive the
//!   fleet key via `TPM2_Duplicate`/`Import`.
//! - `sceau status` (ADR-0003 roadmap Phase 7) — report whether this node
//!   currently holds the fleet key (vs. falling back to its per-node SRK)
//!   and its `key_id`, without starting `serve`. Read-only, no cluster or
//!   network interaction — closes the gap the migration guide's §2c named:
//!   confirming a shared `key_id` across nodes previously required reading
//!   `sceau.service`'s own startup log line.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::certs;

/// Default cap on how many joiners one `enroll` invocation will serve
/// before its listener closes.
pub const DEFAULT_ENROLL_MAX: u32 = 1;

/// Default wall-clock bound on how long an `enroll` listener stays open,
/// regardless of `--max`.
pub const DEFAULT_ENROLL_TIMEOUT_SECS: u64 = 300;

/// Kubernetes KMS v2 plugin that seals data encryption keys with a TPM 2.0.
#[derive(Parser, Debug)]
#[command(name = "sceau", version, about)]
pub struct Cli {
    /// TCTI configuration string for the TPM.
    #[arg(long, global = true, default_value = "device:/dev/tpmrm0")]
    pub tcti: String,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug, PartialEq, Eq)]
pub enum Command {
    /// Serve the Kubernetes KMS v2 plugin over a Unix socket (ADR-0001).
    Serve {
        /// Unix socket kube-apiserver connects to.
        #[arg(long, default_value = "/run/sceau/sceau.sock")]
        socket: PathBuf,
        /// Testing only: never look for the ADR-0003 fleet key, even if
        /// one exists — always use the per-node deterministic SRK, as if
        /// this node had never joined a fleet. Exists to let the
        /// `Decrypt`-only legacy fallback path (Decision 5) be exercised
        /// deliberately, by sealing data under this mode and then
        /// restarting without it. Never use this on a real cluster node
        /// outside of that kind of test.
        #[arg(long, hide = true)]
        force_legacy: bool,
    },
    /// Create the fleet's duplicable sealing key (ADR-0003). Exactly one
    /// node per HA cluster should ever run this without `--force`.
    Genesis {
        /// Delete and recreate the fleet key even if one already exists.
        /// Only ever needed to recover a key that can't be duplicated
        /// (e.g. one created before the `TPM2_Duplicate` `authPolicy` fix)
        /// — forcing a fresh key on an already-enrolled fleet orphans
        /// every other node's copy.
        #[arg(long)]
        force: bool,
    },
    /// Serve a bounded, authenticated `TPM2_Duplicate` to one joining
    /// node, then stop listening (ADR-0003).
    Enroll {
        /// Address to listen on (e.g. `0.0.0.0:8443` — not `9443`, which
        /// collides with k0s's own `k0sApiPort`).
        #[arg(long)]
        listen: String,
        /// Maximum number of joiners to serve before closing the listener.
        #[arg(
            long,
            default_value_t = DEFAULT_ENROLL_MAX,
            value_parser = clap::value_parser!(u32).range(1..)
        )]
        max: u32,
        /// Wall-clock bound, in seconds, on how long the listener stays
        /// open regardless of `--max`.
        #[arg(long, default_value_t = DEFAULT_ENROLL_TIMEOUT_SECS)]
        timeout_secs: u64,
        /// k0s's data directory — used to locate `pki/ca.crt`,
        /// `pki/ca.key`, and `pki/admin.conf`. Override this if k0s was
        /// installed with a non-default `--data-dir`.
        #[arg(long, default_value = certs::K0S_DEFAULT_DATA_DIR)]
        k0s_data_dir: PathBuf,
        /// Node name permitted to receive the fleet key on this invocation.
        /// Repeat for several joiners. Required: cluster membership alone is
        /// not a sufficient bar, since every kubelet in the cluster — workers
        /// included — holds a certificate the same CA signed, and the fleet
        /// key unseals every DEK in the cluster.
        #[arg(long = "allow-node", required = true, value_name = "NAME")]
        allow_node: Vec<String>,
    },
    /// Dial an existing fleet member running `enroll` and receive the
    /// fleet key via `TPM2_Duplicate`/`Import` (ADR-0003).
    Join {
        /// Address of an existing fleet member running `enroll`, as
        /// `<host>:<port>`. `<host>` MUST be the seed's k0s node name (its
        /// `system:node:<name>` identity) — that's the only SAN on the
        /// seed's TLS certificate, and TLS hostname verification falls
        /// back to whatever host is in this address (see ADR-0003
        /// Decision 3's addendum). An IP address here will fail
        /// verification.
        #[arg(long)]
        seed: String,
        /// k0s's data directory — used to locate `pki/ca.crt` and
        /// `kubelet/pki/kubelet-client-current.pem`. Override this if k0s
        /// was installed with a non-default `--data-dir`.
        #[arg(long, default_value = certs::K0S_DEFAULT_DATA_DIR)]
        k0s_data_dir: PathBuf,
    },
    /// Report whether this node currently holds the ADR-0003 fleet key (vs.
    /// its per-node deterministic SRK) and its `key_id`, then exit. Read-only
    /// — never touches the network or the cluster.
    Status,
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod cli_tests;
