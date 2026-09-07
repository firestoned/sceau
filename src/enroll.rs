// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0
//! ADR-0003 `enroll`: the seed side of fleet-key enrollment. Serves
//! exactly one RPC (`Duplicate`) over mTLS, bounded by `--max`
//! and/or `--timeout-secs`, then exits -- there is no long-running
//! enrollment daemon (see the ADR's "Resolved" consequence on one-shot
//! `genesis`/`enroll`/`join` invocations).
//!
//! **A real startup failure found and fixed; not yet re-run live.** This
//! sandbox cannot compile `sceau` at all (no macOS ARM `tss-esapi-sys`
//! support) — every tonic/rustls call here was checked directly against
//! tonic 0.12.3's own source and its `examples/src/tls_client_auth`
//! (fetched, not guessed), and every `fleet`/TPM call reuses signatures
//! already used (and flagged) in `fleet.rs`. The
//! `duplicate_private`/`encrypted_secret`/`encryption_key` marshalling below
//! was fixed after the first real (Linux) compile attempt found it wrong
//! (see CHANGELOG 2026-09-06) — those are raw `TPM2B_*` byte buffers, not
//! `Marshall`-implementing structures, so they go out via `.value().to_vec()`
//! instead. A real `linux/amd64` build (2026-09-06) compiled and linked; the
//! first live `enroll` run on a real node then failed minting its TLS leaf
//! (`certs.rs`'s CA key was PKCS#1, `rcgen` needed PKCS#8 — fixed same day,
//! see CHANGELOG). `cargo test`/`clippy`/`fmt` all pass as of that fix,
//! including a new unit test asserting the exact PKCS#1 key that hit this
//! bug now parses. A second live run past that fix then failed differently:
//! `kube::Client::try_default()` infers config from in-cluster env vars or
//! `~/.kube/config`, neither of which exists when sceau runs as a bare
//! process directly on the controller host — fixed by pointing it at k0s's
//! own admin kubeconfig (`<k0s_data_dir>/pki/admin.conf`) explicitly
//! instead. Neither fix has yet been re-run live — that, plus a full
//! `enroll`/`join` round trip against a real joiner, are both still
//! open. All k0s filesystem paths here are relative to `k0s_data_dir`
//! (default `/var/lib/k0s`, k0s's own default) rather than hardcoded, since
//! k0s's `--data-dir` flag can relocate it.

use std::net::SocketAddr;
use std::path::Path;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use thiserror::Error;
use tokio::sync::Notify;
use tonic::transport::{Certificate, Identity, Server, ServerTlsConfig};
use tonic::{Request, Response, Status};
use tss_esapi::handles::KeyHandle;
use tss_esapi::structures::Public;
use tss_esapi::tcti_ldr::TctiNameConf;
use tss_esapi::traits::{Marshall, UnMarshall};
use tss_esapi::Context;

use crate::authz;
use crate::certs::{self, CertError};
use crate::fleet::{self, FleetError};

// tonic-build's generated `Enroll` trait returns `Result<_, tonic::Status>`
// (176+ bytes) -- idiomatic tonic, not something to box; module-scoped
// since this is generated code we don't otherwise annotate.
#[allow(clippy::result_large_err)]
pub mod pb {
    tonic::include_proto!("enroll.v1");
}
use pb::enroll_server::{Enroll, EnrollServer};
use pb::{DuplicateRequest, DuplicateResponse};

#[derive(Error, Debug)]
pub enum EnrollError {
    #[error("TPM error: {0}")]
    Fleet(#[from] FleetError),
    #[error("TPM error: {0}")]
    Tss(#[from] tss_esapi::Error),
    #[error("certificate error: {0}")]
    Cert(#[from] CertError),
    #[error("reading {0}: {1}")]
    Read(String, std::io::Error),
    #[error("invalid --listen address {0:?}: {1}")]
    InvalidListenAddress(String, std::net::AddrParseError),
    #[error("invalid --tcti {0:?}: {1}")]
    InvalidTcti(String, String),
    #[error("gRPC transport error: {0}")]
    Transport(#[from] tonic::transport::Error),
    #[error("kube client error: {0}")]
    Kube(#[from] kube::Error),
    #[error("reading k0s admin kubeconfig {0:?}: {1}")]
    Kubeconfig(String, kube::config::KubeconfigError),
}

struct EnrollService {
    context: Mutex<Context>,
    fleet_key_handle: KeyHandle,
    kube_client: kube::Client,
    /// Permits remaining. Decremented optimistically in `duplicate`; a
    /// request that finds it already at zero puts its permit back and is
    /// rejected, rather than serving unboundedly past `--max`.
    remaining: AtomicUsize,
    done: Arc<Notify>,
}

fn denied(e: impl std::fmt::Display) -> Status {
    Status::permission_denied(format!("sceau enroll: {e}"))
}

fn internal(e: impl std::fmt::Display) -> Status {
    Status::internal(format!("sceau enroll: {e}"))
}

impl EnrollService {
    /// The actual `Duplicate` logic, split out from [`Enroll::duplicate`] so
    /// that every failure path — auth, authz, unmarshalling, the TPM call
    /// itself — is just an ordinary `?` early return, with permit bookkeeping
    /// handled once by the caller instead of needing its own restore-on-error
    /// arm at every call site.
    #[allow(clippy::result_large_err)]
    async fn handle_duplicate(
        &self,
        request: &Request<DuplicateRequest>,
    ) -> Result<DuplicateResponse, Status> {
        // Decision 3: authentication (mTLS, checked by the TLS layer before
        // this handler even runs) is necessary but not sufficient —
        // authorization is this explicit, separate check against the
        // cluster's own Node objects.
        let peer_certs = request
            .peer_certs()
            .ok_or_else(|| denied("no client certificate presented"))?;
        let leaf = peer_certs
            .first()
            .ok_or_else(|| denied("empty client certificate chain"))?;
        let cn = certs::peer_cert_common_name(leaf).map_err(denied)?;
        let node_name = authz::node_name_from_cn(&cn).map_err(denied)?;
        authz::authorize_node(&self.kube_client, node_name)
            .await
            .map_err(denied)?;
        tracing::info!(node = %node_name, "authorized enrollment request");

        let joiner_public = Public::unmarshall(&request.get_ref().joiner_transport_public)
            .map_err(|e| internal(format!("unmarshalling joiner transport public: {e}")))?;

        let blob = {
            let mut context = self.context.lock().map_err(|e| internal(e.to_string()))?;
            fleet::duplicate_for_joiner(&mut context, self.fleet_key_handle, joiner_public)
                .map_err(internal)?
        };

        let response = DuplicateResponse {
            fleet_key_public: marshall(&blob.fleet_key_public)?,
            // Private/EncryptedSecret/Data are raw TPM2B byte buffers, not
            // ASN.1-style structures -- tss-esapi doesn't implement Marshall
            // for them (only Public/Sensitive/Signature/Attest get that).
            duplicate_private: blob.duplicate_private.value().to_vec(),
            encrypted_secret: blob.encrypted_secret.value().to_vec(),
            has_encryption_key: blob.encryption_key.is_some(),
            encryption_key: match &blob.encryption_key {
                Some(data) => data.value().to_vec(),
                None => Vec::new(),
            },
        };
        tracing::info!(node = %node_name, "served TPM2_Duplicate");
        Ok(response)
    }
}

#[tonic::async_trait]
impl Enroll for EnrollService {
    async fn duplicate(
        &self,
        request: Request<DuplicateRequest>,
    ) -> Result<Response<DuplicateResponse>, Status> {
        let permits_before = self.remaining.fetch_sub(1, Ordering::SeqCst);
        if permits_before == 0 {
            self.remaining.fetch_add(1, Ordering::SeqCst);
            return Err(denied("--max already reached"));
        }
        let was_last_permit = permits_before == 1;

        // A permit was genuinely consumed above (permits_before >= 1) --
        // every failure from here on must give it back. Only a *served*
        // Duplicate (the Ok arm) is allowed to keep it consumed. Without
        // this, any rejected/failed attempt -- an unauthorized node probing,
        // a malformed request, even a transient internal error -- would
        // permanently burn a slot with nothing ever enrolled, and with the
        // default --max=1 that locks out the next legitimate joiner until
        // the operator restarts `enroll`. Confirmed live (2026-09-07): a
        // single rejected enrollment attempt left a second, otherwise-valid
        // attempt failing with "--max already reached".
        match self.handle_duplicate(&request).await {
            Ok(response) => {
                if was_last_permit {
                    self.done.notify_one();
                }
                Ok(Response::new(response))
            }
            Err(status) => {
                self.remaining.fetch_add(1, Ordering::SeqCst);
                Err(status)
            }
        }
    }
}

// tonic::Status (176+ bytes) as an Err-variant is idiomatic for a gRPC
// service helper like this -- boxing it would fight the tonic ecosystem
// convention this whole file otherwise follows, not fix a real problem.
#[allow(clippy::result_large_err)]
fn marshall(value: &impl Marshall) -> Result<Vec<u8>, Status> {
    value.marshall().map_err(|e| internal(e.to_string()))
}

/// `enroll --listen=<addr> [--max=<n>] [--timeout-secs=<d>]`:
/// serve `Duplicate` until `max` requests succeed or `timeout` elapses,
/// whichever comes first, then return. Requires the fleet key to already
/// exist locally (from `genesis` or a prior `join`).
pub async fn run_enroll(
    listen: &str,
    max: u32,
    timeout: Duration,
    tcti: &str,
    k0s_data_dir: &Path,
) -> Result<(), EnrollError> {
    let addr: SocketAddr = listen
        .parse()
        .map_err(|e| EnrollError::InvalidListenAddress(listen.to_string(), e))?;

    let tcti_conf = TctiNameConf::from_str(tcti)
        .map_err(|e| EnrollError::InvalidTcti(tcti.to_string(), e.to_string()))?;
    let mut context = Context::new(tcti_conf)?;
    let fleet_key_handle = fleet::load_fleet_key(&mut context)?;

    let node_name = certs::own_node_name(k0s_data_dir)?;
    let (leaf_cert_pem, leaf_key_pem) =
        certs::mint_enroll_server_identity(&node_name, k0s_data_dir)?;
    let ca_cert_path = k0s_data_dir.join("pki/ca.crt");
    let ca_cert_pem = std::fs::read_to_string(&ca_cert_path)
        .map_err(|e| EnrollError::Read(ca_cert_path.display().to_string(), e))?;

    let tls = ServerTlsConfig::new()
        .identity(Identity::from_pem(leaf_cert_pem, leaf_key_pem))
        .client_ca_root(Certificate::from_pem(ca_cert_pem));

    let admin_kubeconfig_path = k0s_data_dir.join("pki/admin.conf");
    let kubeconfig = kube::config::Kubeconfig::read_from(&admin_kubeconfig_path)
        .map_err(|e| EnrollError::Kubeconfig(admin_kubeconfig_path.display().to_string(), e))?;
    let kube_config = kube::Config::from_custom_kubeconfig(
        kubeconfig,
        &kube::config::KubeConfigOptions::default(),
    )
    .await
    .map_err(|e| EnrollError::Kubeconfig(admin_kubeconfig_path.display().to_string(), e))?;
    let kube_client = kube::Client::try_from(kube_config)?;

    let done = Arc::new(Notify::new());
    let service = EnrollService {
        context: Mutex::new(context),
        fleet_key_handle,
        kube_client,
        remaining: AtomicUsize::new(max as usize),
        done: Arc::clone(&done),
    };

    tracing::info!(%listen, node = %node_name, max, timeout_secs = timeout.as_secs(), "serving enrollment");
    Server::builder()
        .tls_config(tls)?
        .add_service(EnrollServer::new(service))
        .serve_with_shutdown(addr, async move {
            tokio::select! {
                _ = done.notified() => tracing::info!("--max reached, closing listener"),
                _ = tokio::time::sleep(timeout) => tracing::info!("--timeout-secs elapsed, closing listener"),
            }
        })
        .await?;

    Ok(())
}
