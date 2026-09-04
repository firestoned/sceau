// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0

//! TPM 2.0 seal/unseal of data encryption keys.
//!
//! Layout: a deterministic RSA-2048 restricted decryption primary key (the
//! standard SRK template) is recreated in the owner hierarchy at startup.
//! Sealing creates a keyed-hash sealed-data object under that primary; the
//! resulting public+private blobs are the ciphertext. Nothing secret ever
//! exists outside the TPM in plaintext form.

use sha2::{Digest as _, Sha256};
use thiserror::Error;
use tss_esapi::{
    attributes::ObjectAttributesBuilder,
    handles::KeyHandle,
    interface_types::{
        algorithm::{HashingAlgorithm, PublicAlgorithm},
        key_bits::RsaKeyBits,
        resource_handles::Hierarchy,
    },
    structures::{
        Digest, KeyedHashScheme, Private, Public, PublicBuilder, PublicKeyRsa,
        PublicKeyedHashParameters, PublicRsaParametersBuilder, RsaExponent, SensitiveData,
        SymmetricDefinitionObject,
    },
    tcti_ldr::TctiNameConf,
    traits::{Marshall, UnMarshall},
    Context,
};

/// TPM2B_SENSITIVE_DATA is capped at MAX_SYM_DATA (128) bytes. A Kubernetes
/// DEK is 32 bytes, so this is ample.
const MAX_SEAL_DATA: usize = 128;

const ENVELOPE_VERSION: u8 = 1;

#[derive(Error, Debug)]
pub enum TpmError {
    #[error("TPM error: {0}")]
    Tss(#[from] tss_esapi::Error),
    #[error("plaintext too large to seal: {0} bytes (max {MAX_SEAL_DATA})")]
    PlaintextTooLarge(usize),
    #[error("malformed ciphertext envelope")]
    MalformedEnvelope,
}

pub struct TpmSealer {
    context: Context,
    srk: KeyHandle,
    key_id: String,
}

impl TpmSealer {
    /// Connect to the TPM via the given TCTI (e.g. `device:/dev/tpmrm0`) and
    /// (re)create the primary storage key. The key_id is derived from the
    /// primary's public area, so it is stable across reboots of the same TPM.
    pub fn new(tcti: &TctiNameConf) -> Result<Self, TpmError> {
        let mut context = Context::new(tcti.clone())?;
        let (srk, key_id) = context.execute_with_nullauth_session(|ctx| {
            let primary =
                ctx.create_primary(Hierarchy::Owner, srk_public()?, None, None, None, None)?;
            let (_, name, _) = ctx.read_public(primary.key_handle)?;
            let id = hex::encode(Sha256::digest(name.value()))[..16].to_string();
            Ok::<_, TpmError>((primary.key_handle, format!("sceau-{id}")))
        })?;
        Ok(Self {
            context,
            srk,
            key_id,
        })
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// Seal `plaintext` under the SRK and return the KMS ciphertext envelope.
    pub fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, TpmError> {
        if plaintext.len() > MAX_SEAL_DATA {
            return Err(TpmError::PlaintextTooLarge(plaintext.len()));
        }
        let sensitive = SensitiveData::try_from(plaintext.to_vec())?;
        let srk = self.srk;
        let created = self.context.execute_with_nullauth_session(|ctx| {
            let result = ctx.create(srk, sealed_public()?, None, Some(sensitive), None, None)?;
            Ok::<_, TpmError>((result.out_private, result.out_public))
        })?;
        Ok(envelope_encode(&created.1, &created.0))
    }

    /// Unseal a ciphertext envelope produced by [`TpmSealer::seal`].
    pub fn unseal(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, TpmError> {
        let (public, private) = envelope_decode(ciphertext)?;
        let srk = self.srk;
        let data = self.context.execute_with_nullauth_session(|ctx| {
            let handle = ctx.load(srk, private, public)?;
            let data = ctx.unseal(handle.into())?;
            ctx.flush_context(handle.into())?;
            Ok::<_, TpmError>(data)
        })?;
        Ok(data.value().to_vec())
    }
}

impl Drop for TpmSealer {
    fn drop(&mut self) {
        let _ = self
            .context
            .execute_with_nullauth_session(|ctx| ctx.flush_context(self.srk.into()));
    }
}

/// Standard SRK template: RSA-2048, restricted decryption key, fixed to this
/// TPM and its parent, sensitive data generated internally.
fn srk_public() -> Result<Public, TpmError> {
    let attributes = ObjectAttributesBuilder::new()
        .with_fixed_tpm(true)
        .with_fixed_parent(true)
        .with_sensitive_data_origin(true)
        .with_user_with_auth(true)
        .with_decrypt(true)
        .with_restricted(true)
        .build()?;
    Ok(PublicBuilder::new()
        .with_public_algorithm(PublicAlgorithm::Rsa)
        .with_name_hashing_algorithm(HashingAlgorithm::Sha256)
        .with_object_attributes(attributes)
        .with_rsa_parameters(
            PublicRsaParametersBuilder::new_restricted_decryption_key(
                SymmetricDefinitionObject::AES_128_CFB,
                RsaKeyBits::Rsa2048,
                RsaExponent::default(),
            )
            .build()?,
        )
        .with_rsa_unique_identifier(PublicKeyRsa::default())
        .build()?)
}

/// Public area for a sealed-data (keyed hash, null scheme) child object.
fn sealed_public() -> Result<Public, TpmError> {
    let attributes = ObjectAttributesBuilder::new()
        .with_fixed_tpm(true)
        .with_fixed_parent(true)
        .with_user_with_auth(true)
        .build()?;
    Ok(PublicBuilder::new()
        .with_public_algorithm(PublicAlgorithm::KeyedHash)
        .with_name_hashing_algorithm(HashingAlgorithm::Sha256)
        .with_object_attributes(attributes)
        .with_keyed_hash_parameters(PublicKeyedHashParameters::new(KeyedHashScheme::Null))
        .with_keyed_hash_unique_identifier(Digest::default())
        .build()?)
}

/// Ciphertext envelope: `version(1) || public_len(u16 BE) || public || private`.
fn envelope_encode(public: &Public, private: &Private) -> Vec<u8> {
    let public_bytes = public.marshall().expect("SRK-descendant public marshals");
    let mut out = Vec::with_capacity(3 + public_bytes.len() + private.value().len());
    out.push(ENVELOPE_VERSION);
    out.extend_from_slice(&(public_bytes.len() as u16).to_be_bytes());
    out.extend_from_slice(&public_bytes);
    out.extend_from_slice(private.value());
    out
}

fn envelope_decode(ciphertext: &[u8]) -> Result<(Public, Private), TpmError> {
    if ciphertext.len() < 3 || ciphertext[0] != ENVELOPE_VERSION {
        return Err(TpmError::MalformedEnvelope);
    }
    let public_len = u16::from_be_bytes([ciphertext[1], ciphertext[2]]) as usize;
    if ciphertext.len() < 3 + public_len {
        return Err(TpmError::MalformedEnvelope);
    }
    let public = Public::unmarshall(&ciphertext[3..3 + public_len])
        .map_err(|_| TpmError::MalformedEnvelope)?;
    let private = Private::try_from(&ciphertext[3 + public_len..])
        .map_err(|_| TpmError::MalformedEnvelope)?;
    Ok((public, private))
}
