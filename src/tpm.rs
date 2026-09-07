// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0

//! TPM 2.0 seal/unseal of data encryption keys.
//!
//! Layout: sealing creates a keyed-hash sealed-data object under a primary
//! key; the resulting public+private blobs are the ciphertext. Nothing
//! secret ever exists outside the TPM in plaintext form. The primary is
//! either of two things, chosen by the caller (`main.rs`'s `run_serve`):
//!
//! - The default (ADR-0001): a deterministic RSA-2048 restricted decryption
//!   key (the standard SRK template), recreated in the owner hierarchy at
//!   startup — the same key every reboot, unique per TPM.
//! - The ADR-0003 fleet key, when one exists — identical across every
//!   enrolled node by construction (`TPM2_Duplicate`/`Import`), which is
//!   what makes cross-node decrypt possible at all. See
//!   [`TpmSealer::from_primary`] and ADR-0003's Decision 5.

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

// pub(crate) so `src/tpm_tests.rs` can pin the wire-format version.
pub(crate) const ENVELOPE_VERSION: u8 = 1;

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
    primary: KeyHandle,
    key_id: String,
    /// The primary's own `fixedTpm` (== `fixedParent`, by construction of
    /// both [`srk_public`] and the ADR-0003 fleet key's own template) --
    /// every object [`TpmSealer::seal`] creates under it must match, or the
    /// TPM rejects `TPM2_Create` outright with `TPM_RC_ATTRIBUTES`. Derived
    /// from the primary's own `Public` area (already read to compute
    /// `key_id`) rather than passed in by the caller, so it can never
    /// drift out of sync with the actual primary in use.
    child_fixed: bool,
    /// Whether `primary` is a transient object this sealer itself created
    /// (`new`, the per-node SRK) and therefore owns and must
    /// `TPM2_FlushContext` on drop to free the TPM's object slot -- vs. a
    /// reference to an already-persistent object (`from_primary`, the
    /// ADR-0003 fleet key) that must **not** be flushed. See [`Drop`]'s
    /// impl below for why this distinction is load-bearing, not cosmetic.
    owns_transient_primary: bool,
}

impl TpmSealer {
    /// Connect to the TPM via the given TCTI (e.g. `device:/dev/tpmrm0`) and
    /// (re)create the deterministic per-node SRK as the sealing primary
    /// (ADR-0001). The key_id is derived from the primary's public area, so
    /// it is stable across reboots of the same TPM — but unique *per node*,
    /// unlike [`TpmSealer::from_primary`]'s fleet-key case.
    pub fn new(tcti: &TctiNameConf) -> Result<Self, TpmError> {
        let mut context = Context::new(tcti.clone())?;
        let (primary, key_id, child_fixed) = context.execute_with_nullauth_session(|ctx| {
            let primary =
                ctx.create_primary(Hierarchy::Owner, srk_public()?, None, None, None, None)?;
            let (public, name, _) = ctx.read_public(primary.key_handle)?;
            Ok::<_, TpmError>((
                primary.key_handle,
                derive_key_id(name.value()),
                public.object_attributes().fixed_tpm(),
            ))
        })?;
        Ok(Self {
            context,
            primary,
            key_id,
            child_fixed,
            owns_transient_primary: true,
        })
    }

    /// Build a sealer around an already-loaded primary key handle instead of
    /// recreating the deterministic per-node SRK — used by `sceau serve`
    /// once a node holds the ADR-0003 fleet key
    /// (`fleet::load_fleet_key`), so cross-node decrypt actually works (see
    /// ADR-0003 Decision 5). `key_id` is derived the same way as
    /// [`TpmSealer::new`], from the primary's own `Name` — since the fleet
    /// key's public area is identical on every enrolled node by
    /// construction, so is this `key_id`, which is the whole point.
    pub fn from_primary(mut context: Context, primary: KeyHandle) -> Result<Self, TpmError> {
        let (public, name, _) =
            context.execute_with_nullauth_session(|ctx| ctx.read_public(primary))?;
        let key_id = derive_key_id(name.value());
        let child_fixed = public.object_attributes().fixed_tpm();
        Ok(Self {
            context,
            primary,
            key_id,
            child_fixed,
            owns_transient_primary: false,
        })
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// Seal `plaintext` under the sealing primary and return the KMS
    /// ciphertext envelope.
    pub fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, TpmError> {
        if plaintext.len() > MAX_SEAL_DATA {
            return Err(TpmError::PlaintextTooLarge(plaintext.len()));
        }
        let sensitive = SensitiveData::try_from(plaintext.to_vec())?;
        let primary = self.primary;
        let child_fixed = self.child_fixed;
        let created = self.context.execute_with_nullauth_session(|ctx| {
            let result = ctx.create(
                primary,
                sealed_public(child_fixed)?,
                None,
                Some(sensitive),
                None,
                None,
            )?;
            Ok::<_, TpmError>((result.out_private, result.out_public))
        })?;
        Ok(envelope_encode(&created.1, &created.0))
    }

    /// Unseal a ciphertext envelope produced by [`TpmSealer::seal`].
    pub fn unseal(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, TpmError> {
        let (public, private) = envelope_decode(ciphertext)?;
        let primary = self.primary;
        let data = self.context.execute_with_nullauth_session(|ctx| {
            let handle = ctx.load(primary, private, public)?;
            let data = ctx.unseal(handle.into())?;
            ctx.flush_context(handle.into())?;
            Ok::<_, TpmError>(data)
        })?;
        Ok(data.value().to_vec())
    }
}

impl Drop for TpmSealer {
    fn drop(&mut self) {
        // Only flush a primary this sealer actually created (new`'s
        // transient SRK) -- it occupies a real TPM object slot that must be
        // freed. A persistent-derived primary (`from_primary`, the
        // ADR-0003 fleet key) must NOT be flushed: confirmed live
        // (2026-09-07, via `sceau status`, which -- unlike `serve` -- drops
        // its `TpmSealer` immediately instead of holding it for the
        // process's whole lifetime, making this path actually execute for
        // the first time) that `TPM2_FlushContext` on a persistent handle
        // is rejected by real hardware with `TPM_RC` 0x1c4 ("value is out
        // of range or is not correct for the context"), not the harmless
        // ESAPI-local-bookkeeping-only no-op this code previously assumed.
        if !self.owns_transient_primary {
            return;
        }
        let _ = self
            .context
            .execute_with_nullauth_session(|ctx| ctx.flush_context(self.primary.into()));
    }
}

/// `sceau-<first 16 hex chars of SHA256(name)>` — stable for a given TPM
/// object's `Name` (which is itself derived from its public area), so this
/// is stable across reboots for a deterministic key, and identical across
/// nodes for a duplicated one.
fn derive_key_id(name_bytes: &[u8]) -> String {
    let id = hex::encode(Sha256::digest(name_bytes))[..16].to_string();
    format!("sceau-{id}")
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
///
/// `fixed` must match the sealing primary's own `fixedTpm`/`fixedParent`
/// attributes. **Found live** (2026-09-06): the TPM rejects `TPM2_Create`
/// with `TPM_RC_ATTRIBUTES` ("inconsistent attributes") for a
/// `fixedTpm=true` child under a `fixedTpm=false` (duplicable) parent —
/// this is a real TPM 2.0 consistency rule, not a bug in the parent
/// selection itself. The deterministic per-node SRK is `fixedTpm=true`
/// ([`srk_public`]); the ADR-0003 fleet key is deliberately `fixedTpm=false`
/// (that's what makes it duplicable at all) — so a sealed object's own
/// attributes must follow whichever primary [`TpmSealer`] is currently
/// using, not be hardcoded to the ADR-0001 case.
fn sealed_public(fixed: bool) -> Result<Public, TpmError> {
    let attributes = ObjectAttributesBuilder::new()
        .with_fixed_tpm(fixed)
        .with_fixed_parent(fixed)
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
///
/// `pub` so the `fuzz/` crate can build round-trip corpora; this is a pure
/// codec with no TPM state.
pub fn envelope_encode(public: &Public, private: &Private) -> Vec<u8> {
    let public_bytes = public.marshall().expect("SRK-descendant public marshals");
    let mut out = Vec::with_capacity(3 + public_bytes.len() + private.value().len());
    out.push(ENVELOPE_VERSION);
    out.extend_from_slice(&(public_bytes.len() as u16).to_be_bytes());
    out.extend_from_slice(&public_bytes);
    out.extend_from_slice(private.value());
    out
}

/// Parses a ciphertext envelope produced by [`envelope_encode`].
///
/// This is the only parser of attacker-influenced bytes in the seal/unseal
/// path (ciphertext arrives from kube-apiserver), so it is fuzzed by the
/// `fuzz/` crate and must never panic on malformed input.
///
/// # Arguments
/// * `ciphertext` - The envelope bytes to parse
///
/// # Errors
/// Returns `TpmError::MalformedEnvelope` if the envelope is too short, has an
/// unknown version, a truncated public area, or bytes that do not unmarshal
/// into TPM `Public` / `Private` structures.
pub fn envelope_decode(ciphertext: &[u8]) -> Result<(Public, Private), TpmError> {
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

#[cfg(test)]
#[path = "tpm_tests.rs"]
mod tpm_tests;
