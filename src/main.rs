// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Context as _, Result};
use clap::Parser;
use tokio::net::UnixListener;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::Server;
use tracing_subscriber::EnvFilter;
use tss_esapi::{tcti_ldr::TctiNameConf, Context as TpmContext};

use sceau::cli::{Cli, Command};
use sceau::kms::pb::key_management_service_server::KeyManagementServiceServer;
use sceau::{enroll, fleet, join, kms, tpm};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Genesis { force } => run_genesis(&cli.tcti, force),
        Command::Enroll {
            listen,
            max,
            timeout_secs,
            k0s_data_dir,
            allow_node,
        } => enroll::run_enroll(
            &listen,
            max,
            std::time::Duration::from_secs(timeout_secs),
            &cli.tcti,
            &k0s_data_dir,
            &allow_node,
        )
        .await
        .context("enroll failed"),
        Command::Join { seed, k0s_data_dir } => join::run_join(&seed, &cli.tcti, &k0s_data_dir)
            .await
            .context("join failed"),
        Command::Serve {
            socket,
            force_legacy,
        } => run_serve(&cli.tcti, &socket, force_legacy).await,
        Command::Status => run_status(&cli.tcti),
    }
}

/// `sceau serve`: the steady-state KMS v2 Unix-socket plugin (ADR-0001).
async fn run_serve(tcti: &str, socket: &PathBuf, force_legacy: bool) -> Result<()> {
    if let Some(parent) = socket.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    if socket.exists() {
        std::fs::remove_file(socket)
            .with_context(|| format!("removing stale socket {}", socket.display()))?;
    }

    let tcti_conf = TctiNameConf::from_str(tcti).context("invalid TCTI configuration")?;
    let (sealer, legacy_sealer) = if force_legacy {
        tracing::warn!("--force-legacy set: skipping fleet-key detection (testing only)");
        (
            tpm::TpmSealer::new(&tcti_conf).context("initializing TPM sealer")?,
            None,
        )
    } else {
        build_sealer(&tcti_conf).context("initializing TPM sealer")?
    };
    tracing::info!(key_id = %sealer.key_id(), tcti, "TPM primary key ready");

    let listener =
        UnixListener::bind(socket).with_context(|| format!("binding {}", socket.display()))?;
    std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600))?;

    let kms_service = match legacy_sealer {
        Some(legacy) => kms::KmsService::with_legacy_fallback(sealer, legacy),
        None => kms::KmsService::new(sealer),
    };

    tracing::info!(socket = %socket.display(), "serving KMS v2");
    Server::builder()
        .add_service(KeyManagementServiceServer::new(kms_service))
        .serve_with_incoming_shutdown(UnixListenerStream::new(listener), shutdown())
        .await?;

    // Remove the socket so a restart does not trip over it.
    let _ = std::fs::remove_file(socket);
    Ok(())
}

/// ADR-0003 Decision 5: prefer the fleet key over the per-node deterministic
/// SRK whenever one exists, checked once at startup. Any error probing for
/// it (including simply not finding one — the expected, common case for a
/// single-controller/edge deployment that never ran `genesis`) falls back
/// to [`tpm::TpmSealer::new`]'s ADR-0001 behavior unconditionally. When a
/// fleet key *is* found, also builds the legacy per-node SRK as a
/// `Decrypt`-only fallback, so ciphertext sealed before this node joined
/// the fleet stays readable.
fn build_sealer(tcti_conf: &TctiNameConf) -> Result<(tpm::TpmSealer, Option<tpm::TpmSealer>)> {
    let mut probe = TpmContext::new(tcti_conf.clone()).context("connecting to TPM")?;
    match fleet::load_fleet_key(&mut probe) {
        Ok(fleet_key) => {
            tracing::info!("fleet sealing key found — using it instead of the per-node SRK");
            let primary = tpm::TpmSealer::from_primary(probe, fleet_key)?;
            let legacy = tpm::TpmSealer::new(tcti_conf)
                .context("building legacy per-node SRK fallback for Decrypt")?;
            Ok((primary, Some(legacy)))
        }
        Err(_) => {
            tracing::info!("no fleet sealing key found — using the per-node deterministic SRK");
            Ok((tpm::TpmSealer::new(tcti_conf)?, None))
        }
    }
}

/// `sceau genesis` (ADR-0003): create the fleet's duplicable sealing key and
/// exit. Purely local TPM work — no network code path, no interaction with
/// a running k0s cluster or `kube-apiserver` at all (see
/// `docs/migration-ha-existing-cluster.md` §1). Exits `0` on success.
/// Idempotent unless `--force` — see [`fleet::create_fleet_key`]'s doc
/// comment.
fn run_genesis(tcti: &str, force: bool) -> Result<()> {
    let tcti_conf = TctiNameConf::from_str(tcti).context("invalid TCTI configuration")?;
    let mut context = TpmContext::new(tcti_conf).context("connecting to TPM")?;
    fleet::create_fleet_key(&mut context, force).context("creating fleet sealing key")?;
    tracing::info!(force, "fleet sealing key ready");
    Ok(())
}

/// `sceau status` (ADR-0003 roadmap Phase 7): report which key `sceau
/// serve` would use on this node right now, without starting it. Prints
/// `fleet_key=<bool> key_id=<id>` to stdout — plain `key=value` text,
/// deliberately grep/script-friendly, for the migration guide's §2c check
/// ("confirm the logged key_id is identical across every node") without
/// needing to read `sceau.service`'s own startup log line. Mirrors
/// [`build_sealer`]'s own fleet-key-first probe exactly, so this can never
/// report a different answer than what `serve` would actually pick.
fn run_status(tcti: &str) -> Result<()> {
    let tcti_conf = TctiNameConf::from_str(tcti).context("invalid TCTI configuration")?;
    let mut probe = TpmContext::new(tcti_conf.clone()).context("connecting to TPM")?;
    match fleet::load_fleet_key(&mut probe) {
        Ok(fleet_key) => {
            let sealer = tpm::TpmSealer::from_primary(probe, fleet_key)
                .context("reading fleet key's public area")?;
            println!("fleet_key=true key_id={}", sealer.key_id());
        }
        Err(_) => {
            let sealer = tpm::TpmSealer::new(&tcti_conf).context("initializing per-node SRK")?;
            println!("fleet_key=false key_id={}", sealer.key_id());
        }
    }
    Ok(())
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutting down");
}
