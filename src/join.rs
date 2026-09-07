// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0
//! ADR-0003 `join`: the joiner side of fleet-key enrollment. Dials an
//! existing `enroll` seed, presents this node's own k0s-issued kubelet
//! client certificate (Decision 3), and imports + persists the fleet key
//! locally via `fleet::import_and_persist`.
//!
//! **Compiles clean; not yet run end to end.** This sandbox cannot compile
//! `sceau` at all (no macOS ARM `tss-esapi-sys` support) — every tonic/rustls
//! call here was checked directly against tonic 0.12.3's own source
//! (fetched, not guessed), and the `Private`/`EncryptedSecret`/`Data`
//! unmarshalling below was checked against `tss-esapi` 7.7.0's own source
//! after the first real (Linux) compile attempt found it wrong (see
//! CHANGELOG 2026-09-06): those three are raw `TPM2B_*` byte buffers, not
//! one of the `Marshall`/`UnMarshall`-implementing structures, so they
//! round-trip via `TryFrom<Vec<u8>>` instead. A real `linux/amd64` build
//! (2026-09-06) now compiles and links cleanly. Still needs a live
//! `enroll`/`join` round trip against a real seed to confirm end to end.
//! Notably: this node's own kubelet-client cert has no SAN and is
//! deliberately used only as a *client* identity here, never a server
//! identity — see `certs.rs`'s module doc for why that distinction matters.
//! k0s filesystem paths here are relative to `k0s_data_dir` (default
//! `/var/lib/k0s`) rather than hardcoded, since k0s's `--data-dir` flag can
//! relocate it.

use std::path::Path;
use std::str::FromStr;

use thiserror::Error;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};
use tss_esapi::structures::{Data, EncryptedSecret, Private, Public};
use tss_esapi::tcti_ldr::TctiNameConf;
use tss_esapi::traits::{Marshall, UnMarshall};
use tss_esapi::Context;

use crate::certs::CertError;
use crate::fleet::{self, DuplicationBlob, FleetError};

// See enroll.rs's identical `pb` module for why this is allowed here too --
// tonic::Status as an Err-variant is idiomatic, not a real problem to fix.
#[allow(clippy::result_large_err)]
pub mod pb {
    tonic::include_proto!("enroll.v1");
}
use pb::enroll_client::EnrollClient;
use pb::DuplicateRequest;

#[derive(Error, Debug)]
pub enum JoinError {
    #[error("TPM error: {0}")]
    Fleet(#[from] FleetError),
    #[error("TPM error: {0}")]
    Tss(#[from] tss_esapi::Error),
    #[error("certificate error: {0}")]
    Cert(#[from] CertError),
    #[error("reading {0}: {1}")]
    Read(String, std::io::Error),
    #[error("invalid --tcti {0:?}: {1}")]
    InvalidTcti(String, String),
    #[error("invalid --seed address {0:?}: {1}")]
    InvalidSeedUri(String, http::uri::InvalidUri),
    #[error("connecting to seed {0:?}: {1}")]
    Connect(String, tonic::transport::Error),
    #[error("seed rejected enrollment request: {0}")]
    Rejected(#[from] tonic::Status),
    #[error("unmarshalling seed's response: {0}")]
    Unmarshall(tss_esapi::Error),
}

/// `join --seed=<host:port>`: create this node's one-time transport key,
/// dial `seed`'s `enroll` listener, exchange it for a `TPM2_Duplicate`d
/// copy of the fleet key, and persist the imported result locally. Per
/// systemd ordering requirement (ADR-0003 Decision 2): must run after this
/// node's own k0s controller-join completes, since the client cert used
/// here doesn't exist before that.
// `JoinError::Rejected(tonic::Status)` makes the whole enum 176+ bytes --
// idiomatic for a tonic-client caller, not worth boxing just to shrink it.
#[allow(clippy::result_large_err)]
pub async fn run_join(seed: &str, tcti: &str, k0s_data_dir: &Path) -> Result<(), JoinError> {
    let tcti_conf = TctiNameConf::from_str(tcti)
        .map_err(|e| JoinError::InvalidTcti(tcti.to_string(), e.to_string()))?;
    let mut context = Context::new(tcti_conf)?;
    let (transport_handle, transport_public) = fleet::create_transport_key(&mut context)?;

    let client_cert_path = k0s_data_dir.join("kubelet/pki/kubelet-client-current.pem");
    let ca_cert_path = k0s_data_dir.join("pki/ca.crt");
    let client_pem = std::fs::read_to_string(&client_cert_path)
        .map_err(|e| JoinError::Read(client_cert_path.display().to_string(), e))?;
    let ca_cert_pem = std::fs::read_to_string(&ca_cert_path)
        .map_err(|e| JoinError::Read(ca_cert_path.display().to_string(), e))?;

    let tls = ClientTlsConfig::new()
        .ca_certificate(Certificate::from_pem(ca_cert_pem))
        .identity(Identity::from_pem(client_pem.clone(), client_pem));

    let uri = format!("https://{seed}");
    let endpoint = Channel::from_shared(uri.clone())
        .map_err(|e| JoinError::InvalidSeedUri(uri.clone(), e))?
        .tls_config(tls)
        .map_err(|e| JoinError::Connect(uri.clone(), e))?;
    let channel = endpoint
        .connect()
        .await
        .map_err(|e| JoinError::Connect(uri.clone(), e))?;

    tracing::info!(seed, "dialing enrollment seed");
    let mut client = EnrollClient::new(channel);
    let request = DuplicateRequest {
        joiner_transport_public: marshall(&transport_public)?,
    };
    let response = client.duplicate(request).await?.into_inner();

    let blob = DuplicationBlob {
        fleet_key_public: Public::unmarshall(&response.fleet_key_public)
            .map_err(JoinError::Unmarshall)?,
        duplicate_private: Private::try_from(response.duplicate_private)
            .map_err(JoinError::Unmarshall)?,
        encrypted_secret: EncryptedSecret::try_from(response.encrypted_secret)
            .map_err(JoinError::Unmarshall)?,
        encryption_key: if response.has_encryption_key {
            Some(Data::try_from(response.encryption_key).map_err(JoinError::Unmarshall)?)
        } else {
            None
        },
    };

    fleet::import_and_persist(&mut context, transport_handle, blob)?;
    tracing::info!("fleet sealing key imported and persisted");
    Ok(())
}

// Same rationale as `run_join` above -- see its comment.
#[allow(clippy::result_large_err)]
fn marshall(value: &impl Marshall) -> Result<Vec<u8>, JoinError> {
    value.marshall().map_err(JoinError::Tss)
}
