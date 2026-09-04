// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0

mod kms;
mod tpm;

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Context as _, Result};
use clap::Parser;
use tokio::net::UnixListener;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::Server;
use tracing_subscriber::EnvFilter;
use tss_esapi::tcti_ldr::TctiNameConf;

use kms::pb::key_management_service_server::KeyManagementServiceServer;

/// Kubernetes KMS v2 plugin that seals data encryption keys with a TPM 2.0.
#[derive(Parser)]
#[command(name = "sceau", version, about)]
struct Args {
    /// Unix socket kube-apiserver connects to.
    #[arg(long, default_value = "/run/sceau/sceau.sock")]
    socket: PathBuf,

    /// TCTI configuration string for the TPM.
    #[arg(long, default_value = "device:/dev/tpmrm0")]
    tcti: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let args = Args::parse();

    if let Some(parent) = args.socket.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    if args.socket.exists() {
        std::fs::remove_file(&args.socket)
            .with_context(|| format!("removing stale socket {}", args.socket.display()))?;
    }

    let tcti = TctiNameConf::from_str(&args.tcti).context("invalid TCTI configuration")?;
    let sealer = tpm::TpmSealer::new(&tcti).context("initializing TPM sealer")?;
    tracing::info!(key_id = %sealer.key_id(), tcti = %args.tcti, "TPM primary key ready");

    let listener = UnixListener::bind(&args.socket)
        .with_context(|| format!("binding {}", args.socket.display()))?;
    std::fs::set_permissions(&args.socket, std::fs::Permissions::from_mode(0o600))?;

    tracing::info!(socket = %args.socket.display(), "serving KMS v2");
    Server::builder()
        .add_service(KeyManagementServiceServer::new(kms::KmsService::new(
            sealer,
        )))
        .serve_with_incoming_shutdown(UnixListenerStream::new(listener), shutdown())
        .await?;

    // Remove the socket so a restart does not trip over it.
    let _ = std::fs::remove_file(&args.socket);
    Ok(())
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutting down");
}
