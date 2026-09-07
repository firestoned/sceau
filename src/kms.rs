// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0

//! Kubernetes KMS v2 gRPC service backed by the TPM sealer.

use std::sync::Mutex;

use tonic::{Request, Response, Status};

use crate::tpm::TpmSealer;

pub mod pb {
    tonic::include_proto!("v2");
}

use pb::key_management_service_server::KeyManagementService;
use pb::{
    DecryptRequest, DecryptResponse, EncryptRequest, EncryptResponse, StatusRequest, StatusResponse,
};

pub struct KmsService {
    /// The TPM is a single-threaded resource; serialize all commands.
    sealer: Mutex<TpmSealer>,
    /// ADR-0003 Decision 5: set only when `sealer` is the fleet key — the
    /// per-node deterministic SRK a node used *before* joining a fleet.
    /// Tried only as a `Decrypt` fallback, for ciphertext sealed under it
    /// prior to the switch; never used for `Encrypt`, since all new data
    /// should go under the preferred (fleet) key.
    legacy_sealer: Option<Mutex<TpmSealer>>,
}

impl KmsService {
    pub fn new(sealer: TpmSealer) -> Self {
        Self {
            sealer: Mutex::new(sealer),
            legacy_sealer: None,
        }
    }

    /// `sealer` is preferred for everything; `legacy_sealer` is a `Decrypt`-only
    /// fallback for pre-fleet-join ciphertext (ADR-0003 Decision 5).
    pub fn with_legacy_fallback(sealer: TpmSealer, legacy_sealer: TpmSealer) -> Self {
        Self {
            sealer: Mutex::new(sealer),
            legacy_sealer: Some(Mutex::new(legacy_sealer)),
        }
    }

    fn key_id(&self) -> String {
        self.sealer
            .lock()
            .map(|s| s.key_id().to_string())
            .unwrap_or_default()
    }
}

fn internal(e: impl std::fmt::Display) -> Status {
    // Do not leak TPM internals to the apiserver beyond the error class.
    Status::internal(format!("sceau: {e}"))
}

#[tonic::async_trait]
impl KeyManagementService for KmsService {
    async fn status(
        &self,
        _request: Request<StatusRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        Ok(Response::new(StatusResponse {
            version: "v2".into(),
            healthz: "ok".into(),
            key_id: self.key_id(),
        }))
    }

    async fn encrypt(
        &self,
        request: Request<EncryptRequest>,
    ) -> Result<Response<EncryptResponse>, Status> {
        let req = request.into_inner();
        let mut sealer = self.sealer.lock().map_err(internal)?;
        tracing::info!(uid = %req.uid, key_id = %sealer.key_id(), "sealing DEK");
        let ciphertext = sealer.seal(&req.plaintext).map_err(internal)?;
        Ok(Response::new(EncryptResponse {
            ciphertext,
            key_id: sealer.key_id().into(),
            annotations: Default::default(),
        }))
    }

    async fn decrypt(
        &self,
        request: Request<DecryptRequest>,
    ) -> Result<Response<DecryptResponse>, Status> {
        let req = request.into_inner();

        {
            let mut sealer = self.sealer.lock().map_err(internal)?;
            if req.key_id == sealer.key_id() {
                tracing::info!(uid = %req.uid, key_id = %req.key_id, "unsealing DEK");
                let plaintext = sealer.unseal(&req.ciphertext).map_err(internal)?;
                return Ok(Response::new(DecryptResponse { plaintext }));
            }
        }

        // ADR-0003 Decision 5: a key_id that doesn't match the preferred
        // sealer might still be this node's own pre-fleet-join ciphertext.
        if let Some(legacy) = &self.legacy_sealer {
            let mut legacy_sealer = legacy.lock().map_err(internal)?;
            if req.key_id == legacy_sealer.key_id() {
                tracing::info!(uid = %req.uid, key_id = %req.key_id, "unsealing DEK (legacy per-node key)");
                let plaintext = legacy_sealer.unseal(&req.ciphertext).map_err(internal)?;
                return Ok(Response::new(DecryptResponse { plaintext }));
            }
        }

        Err(Status::invalid_argument(format!(
            "unknown key_id {}",
            req.key_id
        )))
    }
}
