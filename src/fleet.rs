// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0
//! ADR-0003 fleet-key duplication primitives: create the fleet's duplicable
//! sealing key (`genesis`), wrap it for a joining node (seed side of
//! `enroll`), and unwrap + persist it locally (`join`).
//!
//! **Unverified in this session.** This sandbox cannot compile `sceau` at
//! all (`tss-esapi-sys` has no macOS ARM support, no local `tpm2-tss` to
//! fall back to bindgen against) — every `Context` method call here was
//! checked against the crate's published docs.rs signatures. Two specific
//! points could not be confirmed that way and need checking first when this
//! is built on real hardware/CI:
//!
//! 1. [`FLEET_KEY_PERSISTENT_HANDLE`]'s construction via
//!    `PersistentTpmHandle::new` and the exact module path of `Persistent`
//!    (the `evict_control` parameter type, distinct from
//!    `PersistentTpmHandle`) — both used in [`persist`].
//! 2. ~~The `ObjectHandle` → `KeyHandle` conversions in [`persist`] and
//!    [`load_fleet_key`]~~ — confirmed via a real compile (2026-09-06) that
//!    `tss-esapi` 7.7.0 provides an infallible `From`, not `TryFrom`; fixed
//!    (a `clippy::unnecessary_fallible_conversions` catch, see CHANGELOG).
//!
//! Everything else — `create_primary`, `read_public`, `load_external_public`,
//! `duplicate`, `import`, `load`, `flush_context` — was checked directly
//! against `Context`'s documented signatures, including argument order and
//! `ObjectHandle` vs. `KeyHandle` parameter types.
//!
//! **A real, live `TPM2_Duplicate` failure and fix (2026-09-06):** the first
//! live `enroll`/`join` attempt failed with `TPM_RC_AUTH_TYPE`
//! ("authorization handle is not correct for command") — `TPM2_Duplicate`
//! requires ADMIN-role authorization on the object being duplicated, which
//! the TPM only ever accepts via a *policy* session, never the plain
//! HMAC session `execute_with_nullauth_session` builds, regardless of the
//! object's `userWithAuth` setting or its auth value being empty. Fixed by
//! reproducing `tss-esapi`'s own doc-tested example for `Context::duplicate`
//! verbatim: [`duplicable_storage_public`] now bakes a `PolicyAuthValue` +
//! `PolicyCommandCode(Duplicate)` policy into every key's `authPolicy` at
//! creation ([`duplication_auth_policy_digest`]), and
//! [`duplicate_for_joiner`] authorizes the real call with a matching
//! `SessionType::Policy` session ([`duplication_auth_policy_session`])
//! instead of a nullauth one. **This changes what `genesis` creates** — a
//! fleet key persisted before this fix has no `authPolicy` and cannot be
//! duplicated; it must be re-created (clear the persistent handle, re-run
//! `genesis`) before `enroll` can succeed against it.
//!
//! Protocol (ADR-0003 Decision 2, as revised — the wrapping target is a
//! fresh plain duplicable key the joiner creates, not the vTPM's
//! Endorsement Key directly; see the ADR's amended Consequences for why):
//!
//! 1. Genesis: [`create_fleet_key`] on the first node, persisted via
//!    `TPM2_EvictControl` so it survives restarts (it is a randomly
//!    generated key, not deterministically recreated like the SRK).
//! 2. Joiner: [`create_transport_key`] — a one-time-use duplicable key;
//!    only its `Public` area is ever sent anywhere.
//! 3. Seed (serving `enroll`): [`duplicate_for_joiner`] — loads the
//!    joiner's transport key `Public` as an external object (no private
//!    material involved on the seed's side) and duplicates the fleet key
//!    against it.
//! 4. Joiner: [`import_and_persist`] — imports the duplicate under its own
//!    (real, locally-loaded) transport key, loads the result, and persists
//!    it to [`FLEET_KEY_PERSISTENT_HANDLE`]. The transport key itself is
//!    flushed afterward — it was only ever a one-time envelope.

use thiserror::Error;
use tss_esapi::{
    attributes::{ObjectAttributesBuilder, SessionAttributesBuilder},
    constants::{CommandCode, SessionType},
    handles::{KeyHandle, ObjectHandle, PersistentTpmHandle, SessionHandle, TpmHandle},
    interface_types::{
        algorithm::{HashingAlgorithm, PublicAlgorithm},
        dynamic_handles::Persistent,
        key_bits::RsaKeyBits,
        resource_handles::{Hierarchy, Provision},
        session_handles::{AuthSession, PolicySession},
    },
    structures::{
        Data, Digest, EncryptedSecret, Private, Public, PublicBuilder, PublicKeyRsa,
        PublicRsaParametersBuilder, RsaExponent, SymmetricDefinition, SymmetricDefinitionObject,
    },
    Context,
};

use crate::tpm::TpmError;

/// Well-known persistent handle the fleet key is stored at, once imported
/// (or created, on the genesis node). Chosen in the TPM's owner-application
/// persistent range (`0x81` high byte); deliberately distinct from
/// `0x81010001`, the reserved well-known RSA Endorsement Key handle, to
/// avoid any collision with a platform-provisioned EK.
const FLEET_KEY_PERSISTENT_HANDLE: u32 = 0x8102_0001;

#[derive(Error, Debug)]
pub enum FleetError {
    #[error("TPM error: {0}")]
    Tpm(#[from] TpmError),
    #[error("TPM error: {0}")]
    Tss(#[from] tss_esapi::Error),
    #[error("TPM returned no session handle when starting an auth session")]
    NoSessionHandle,
}

/// Result of [`duplicate_for_joiner`] — everything a joiner needs to
/// `TPM2_Import` the fleet key locally. `fleet_key_public` travels alongside
/// so the joiner (which has never seen the fleet key before) can `load()`
/// it after import.
pub struct DuplicationBlob {
    pub fleet_key_public: Public,
    pub duplicate_private: Private,
    pub encrypted_secret: EncryptedSecret,
    pub encryption_key: Option<Data>,
}

/// The symmetric algorithm used to protect the duplication blob in transit.
/// Matches the algorithm already used for the SRK's own key derivation in
/// `tpm.rs`, for consistency, not because a different choice would be wrong.
fn duplication_symmetric_alg() -> SymmetricDefinitionObject {
    SymmetricDefinitionObject::AES_128_CFB
}

/// Public area for a plain, duplicable RSA storage key: restricted
/// decryption, `sensitiveDataOrigin`, but — unlike `tpm.rs`'s SRK template —
/// `fixedTpm`/`fixedParent` are both *clear*, which is what makes an object
/// eligible for `TPM2_Duplicate` at all.
///
/// Also carries [`duplication_auth_policy_digest`] as its `authPolicy`.
/// **Found live** (2026-09-06, real hardware): `TPM2_Duplicate` requires
/// ADMIN-role authorization on the object being duplicated, and the TPM
/// only ever accepts ADMIN-role authorization via a policy session — never
/// a plain password/HMAC session, regardless of `userWithAuth` or whether
/// the auth value is empty. A fresh HMAC session (exactly what
/// `execute_with_nullauth_session` builds) was rejected with
/// `TPM_RC_AUTH_TYPE` ("authorization handle is not correct for command").
/// Baking this policy into *every* key this function builds (not just the
/// fleet key) is harmless for the joiner's transport key — it's only ever
/// used for `Import`'s USER-role auth on `parentHandle`, still satisfied by
/// its `userWithAuth=true` HMAC session regardless of `authPolicy` being
/// set — and means either object could, in principle, later be
/// re-duplicated itself.
fn duplicable_storage_public(context: &mut Context) -> Result<Public, FleetError> {
    let auth_policy = duplication_auth_policy_digest(context)?;
    let attributes = ObjectAttributesBuilder::new()
        .with_sensitive_data_origin(true)
        .with_user_with_auth(true)
        .with_decrypt(true)
        .with_restricted(true)
        .build()?;
    Ok(PublicBuilder::new()
        .with_public_algorithm(PublicAlgorithm::Rsa)
        .with_name_hashing_algorithm(HashingAlgorithm::Sha256)
        .with_object_attributes(attributes)
        .with_auth_policy(auth_policy)
        .with_rsa_parameters(
            PublicRsaParametersBuilder::new_restricted_decryption_key(
                duplication_symmetric_alg(),
                RsaKeyBits::Rsa2048,
                RsaExponent::default(),
            )
            .build()?,
        )
        .with_rsa_unique_identifier(PublicKeyRsa::default())
        .build()?)
}

/// The exact policy `duplicate_for_joiner` must reproduce at use time via
/// [`duplication_auth_policy_session`]: `PolicyCommandCode(Duplicate)` alone
/// — restricts this policy to authorizing `TPM2_Duplicate` specifically,
/// nothing else. Computed here via a `Trial` session since we need the
/// resulting digest, not to actually authorize anything yet.
///
/// **Deliberately omits `PolicyAuthValue`**, unlike `tss-esapi`'s own
/// doc-tested example for `Context::duplicate` — found live (2026-09-06)
/// that including it produces `TPM_RC_POLICY_FAIL` when the object handle
/// being authorized comes from `tr_from_tpm_public` (as
/// [`load_fleet_key`]'s does) rather than a handle ESAPI just created
/// itself: `PolicyAuthValue` needs the ESYS context to know the object's
/// real auth value to fold into the session's HMAC, which a freshly
/// looked-up `ObjectHandle` has no way to know unless `tr_set_auth` is
/// called (this codebase never does). Since the fleet key's auth value is
/// intentionally empty, proving knowledge of it adds no real restriction
/// anyway — `PolicyCommandCode` alone is both sufficient for what this
/// policy needs to express and independent of any auth-value plumbing.
fn duplication_auth_policy_digest(context: &mut Context) -> Result<Digest, FleetError> {
    let trial_session = start_session(context, SessionType::Trial)?;
    let policy_session =
        PolicySession::try_from(trial_session).map_err(|_| FleetError::NoSessionHandle)?;
    context.policy_command_code(policy_session, CommandCode::Duplicate)?;
    let digest = context.policy_get_digest(policy_session)?;
    context.flush_context(SessionHandle::from(trial_session).into())?;
    Ok(digest)
}

/// A real (non-`Trial`) policy session satisfying
/// [`duplication_auth_policy_digest`], for actually authorizing a
/// `TPM2_Duplicate` call — set it as the ambient session via
/// `execute_with_session` before calling `duplicate()`. The caller owns
/// flushing it afterward (mirrors `execute_with_nullauth_session`'s own
/// cleanup, which this can't reuse since it needs `SessionType::Policy`,
/// not `SessionType::Hmac`).
fn duplication_auth_policy_session(context: &mut Context) -> Result<AuthSession, FleetError> {
    let session = start_session(context, SessionType::Policy)?;
    let policy_session =
        PolicySession::try_from(session).map_err(|_| FleetError::NoSessionHandle)?;
    context.policy_command_code(policy_session, CommandCode::Duplicate)?;
    Ok(session)
}

/// Shared setup for both policy-session helpers above (and
/// [`import_and_persist`]'s own session): start a session of the given
/// type, with the same decrypt/encrypt attributes
/// `execute_with_nullauth_session` already uses for its own HMAC session,
/// plus `continueSession=true`.
///
/// **Found live** (2026-09-06, real hardware): a TPM session defaults to
/// `continueSession=false`, meaning the TPM flushes it automatically after
/// *any* command that references it in the session area — not just ones
/// that use it for handle authorization. `duplicate_for_joiner` shares one
/// session across three ESAPI calls (`load_external_public`, `duplicate`,
/// `flush_context`); `load_external_public` alone (no auth needed, but
/// still passes the ambient session for parameter encryption) was enough to
/// auto-flush the session before `duplicate()` ever ran, producing
/// `TPM_RC_POLICY_FAIL` — a digest mismatch that was really "the session
/// this digest belongs to doesn't exist anymore," not a wrong policy.
/// `execute_with_nullauth_session` (used everywhere else in this file for a
/// *single* TPM call per session) has the same default and is fine for
/// that reason alone; anywhere multiple calls share one session, this
/// helper — not that one — must be used instead.
fn start_session(
    context: &mut Context,
    session_type: SessionType,
) -> Result<AuthSession, FleetError> {
    let session = context
        .start_auth_session(
            None,
            None,
            None,
            session_type,
            SymmetricDefinition::AES_128_CFB,
            HashingAlgorithm::Sha256,
        )?
        .ok_or(FleetError::NoSessionHandle)?;
    let (attributes, mask) = SessionAttributesBuilder::new()
        .with_continue_session(true)
        .with_decrypt(true)
        .with_encrypt(true)
        .build();
    context.tr_sess_set_attributes(session, attributes, mask)?;
    Ok(session)
}

/// `sceau genesis`: create the fleet's duplicable sealing key under the
/// Owner hierarchy and persist it immediately — unlike the SRK, this key is
/// randomly generated, not deterministically recreated from a template, so
/// losing the transient handle without persisting it would lose the key.
/// Returns the persistent handle it now lives at.
///
/// Idempotent by default: if a fleet key already exists at
/// [`FLEET_KEY_PERSISTENT_HANDLE`] (e.g. `genesis` run twice, or run again
/// after a `join`), returns the existing handle rather than generating and
/// persisting a second key — sceau must never silently replace the fleet's
/// shared sealing key out from under an already-enrolled quorum.
///
/// `force`: delete any existing fleet key first and create a fresh one
/// regardless. Needed for a key persisted before the `TPM2_Duplicate`
/// `authPolicy` fix (2026-09-06, see this module's own doc comment) — such
/// a key has no `authPolicy` and can never be duplicated under any session
/// type, so idempotent "reuse what's there" is actively wrong for it. This
/// is deliberately opt-in and not the default: forcing a fresh key on an
/// already-enrolled multi-node fleet orphans every other node's copy.
pub fn create_fleet_key(context: &mut Context, force: bool) -> Result<KeyHandle, FleetError> {
    if force {
        delete_persisted_fleet_key(context)?;
    } else if let Ok(existing) = load_fleet_key(context) {
        return Ok(existing);
    }
    let public = duplicable_storage_public(context)?;
    let created = context.execute_with_nullauth_session(|ctx| {
        ctx.create_primary(Hierarchy::Owner, public.clone(), None, None, None, None)
    })?;
    persist(context, created.key_handle)
}

/// Remove any fleet key currently persisted at
/// [`FLEET_KEY_PERSISTENT_HANDLE`], if one exists. A no-op if none does.
/// `TPM2_EvictControl` deletes a persistent object when given its own
/// current persistent handle as both the object reference and the target
/// location (the same command [`persist`] uses to *create* a persistent
/// copy, just fed an already-persistent object instead of a fresh
/// transient one).
fn delete_persisted_fleet_key(context: &mut Context) -> Result<(), FleetError> {
    let Ok(existing) = load_fleet_key(context) else {
        return Ok(());
    };
    let persistent_handle = PersistentTpmHandle::new(FLEET_KEY_PERSISTENT_HANDLE)?;
    context.execute_with_nullauth_session(|ctx| {
        ctx.evict_control(
            Provision::Owner,
            existing.into(),
            Persistent::Persistent(persistent_handle),
        )
    })?;
    Ok(())
}

/// `join` step 2: create this node's one-time-use transport key. Only
/// `Public` (the second element) is ever sent to a seed — the private half
/// never leaves this TPM.
pub fn create_transport_key(context: &mut Context) -> Result<(KeyHandle, Public), FleetError> {
    let public = duplicable_storage_public(context)?;
    let created = context.execute_with_nullauth_session(|ctx| {
        ctx.create_primary(Hierarchy::Owner, public, None, None, None, None)
    })?;
    let (public, _name, _qualified_name) =
        context.execute_with_nullauth_session(|ctx| ctx.read_public(created.key_handle))?;
    Ok((created.key_handle, public))
}

/// `enroll` (seed side): wrap the fleet key for `joiner_transport_public`.
/// No private material from the joiner is ever involved here — its
/// transport key's `Public` area is loaded as an external, unauthenticated
/// object purely to serve as `TPM2_Duplicate`'s wrapping target.
pub fn duplicate_for_joiner(
    context: &mut Context,
    fleet_key_handle: KeyHandle,
    joiner_transport_public: Public,
) -> Result<DuplicationBlob, FleetError> {
    let (fleet_key_public, _name, _qualified_name) =
        context.execute_with_nullauth_session(|ctx| ctx.read_public(fleet_key_handle))?;

    // Load the joiner's transport key's Public as an external object FIRST,
    // with NO session attached at all. LoadExternal needs no authorization
    // — but found live (2026-09-06) that merely giving it the *same*
    // ambient policy session `duplicate()` below needs was enough to make
    // `duplicate()` fail with TPM_RC_POLICY_FAIL, despite independently
    // confirming (via a diagnostic policy_get_digest call, since removed)
    // that the session's digest correctly matched the object's stored
    // authPolicy right before the call. The exact TPM-internal mechanism
    // wasn't root-caused — nonce rolling and nonce-derived per-command
    // ordering are one real TPM 2.0 property `execute_with_session` doesn't
    // shield callers from, and are the leading suspect — but the fix
    // doesn't require knowing the mechanism: never let this policy session
    // touch any command except the one it exists to authorize.
    let external = context.execute_without_session(|ctx| {
        ctx.load_external_public(joiner_transport_public, Hierarchy::Owner)
    })?;

    // TPM2_Duplicate needs ADMIN-role authorization on fleet_key_handle,
    // which only a policy session can provide — see duplicable_storage_public's
    // doc comment for why execute_with_nullauth_session (HMAC) doesn't work
    // here, unlike everywhere else in this file. This session is used for
    // nothing else (see above).
    let policy_session = duplication_auth_policy_session(context)?;
    let result = context.execute_with_session(Some(policy_session), |ctx| {
        ctx.duplicate(
            fleet_key_handle.into(),
            external.into(),
            None,
            duplication_symmetric_alg(),
        )
    });
    context.flush_context(SessionHandle::from(policy_session).into())?;
    // Best-effort: the external object is only a wrapping target, never
    // referenced again after duplicate() returns, regardless of outcome.
    let _ = context.execute_without_session(|ctx| ctx.flush_context(external.into()));
    let (encryption_key, duplicate_private, encrypted_secret) = result?;

    Ok(DuplicationBlob {
        fleet_key_public,
        duplicate_private,
        encrypted_secret,
        // duplicate()'s first return value is a plain Data, not Option<Data>
        // (confirmed by the real compiler, not assumed) — DuplicationBlob
        // stores it as Option<Data> only because import()'s matching
        // parameter is optional; wrap it here, at the one point it's
        // produced, rather than changing the field's type.
        encryption_key: Some(encryption_key),
    })
}

/// `join` step 4: import the fleet key under this node's own transport
/// key (created by [`create_transport_key`], genuinely loaded locally —
/// unlike the seed's external-object load, this one has real private
/// material behind it, which is what actually unwraps the duplicate), load
/// the result, persist it, and flush the now-unneeded transport key.
/// Returns the fleet key's new persistent handle.
///
/// Always evicts whatever already occupies [`FLEET_KEY_PERSISTENT_HANDLE`]
/// first, unconditionally — unlike [`create_fleet_key`]'s opt-in `force`.
/// `join` is only ever invoked because the operator wants this node's fleet
/// key slot to now hold the key it's about to receive (a rejoin after a
/// restart, or recovering from a previous partial join), so there is no
/// "protect what's already there" case the way there is for `genesis`.
/// Confirmed live (2026-09-07): without this, `persist`'s `TPM2_EvictControl`
/// fails with `TPM_RC_NV_DEFINED` ("NV Index or persistent object already
/// defined") whenever this node already holds any object at that handle —
/// exactly the gap the roadmap's Phase 7 flagged as unverified.
pub fn import_and_persist(
    context: &mut Context,
    transport_key_handle: KeyHandle,
    blob: DuplicationBlob,
) -> Result<KeyHandle, FleetError> {
    // Not execute_with_nullauth_session: import() then load() both need
    // USER-role auth on transport_key_handle, and that helper's session
    // defaults to continueSession=false -- see start_session's doc comment
    // for why chaining two auth-requiring calls on such a session fails.
    let session = start_session(context, SessionType::Hmac)?;
    let result = context.execute_with_session(Some(session), |ctx| {
        let imported_private = ctx.import(
            transport_key_handle.into(),
            blob.encryption_key,
            blob.fleet_key_public.clone(),
            blob.duplicate_private,
            blob.encrypted_secret,
            duplication_symmetric_alg(),
        )?;
        ctx.load(
            transport_key_handle,
            imported_private,
            blob.fleet_key_public,
        )
    });
    context.flush_context(SessionHandle::from(session).into())?;
    let fleet_key_handle = result?;
    delete_persisted_fleet_key(context)?;
    let persisted = persist(context, fleet_key_handle)?;
    // The transport key was only ever a one-time envelope for this
    // duplication — nothing needs it once the fleet key is persisted.
    context.execute_with_nullauth_session(|ctx| ctx.flush_context(transport_key_handle.into()))?;
    Ok(persisted)
}

/// Persist a transient key handle to [`FLEET_KEY_PERSISTENT_HANDLE`], so it
/// survives a restart without needing to re-run genesis/join.
///
/// **Not idempotent by itself** — confirmed live (2026-09-07): calling this
/// while the handle is already occupied fails outright with
/// `TPM_RC_NV_DEFINED` ("NV Index or persistent object already defined"),
/// it does not silently overwrite or no-op. Every caller in this module
/// ([`create_fleet_key`]'s `force` path, [`import_and_persist`]) is
/// responsible for calling [`delete_persisted_fleet_key`] first when
/// replacing an existing key is actually intended.
fn persist(context: &mut Context, transient: KeyHandle) -> Result<KeyHandle, FleetError> {
    let persistent_handle = PersistentTpmHandle::new(FLEET_KEY_PERSISTENT_HANDLE)?;
    let handle = context.execute_with_nullauth_session(|ctx| {
        ctx.evict_control(
            Provision::Owner,
            transient.into(),
            Persistent::Persistent(persistent_handle),
        )
    })?;
    key_handle_from(handle)
}

/// Load the fleet key from its well-known persistent handle — used by
/// steady-state `sceau` (once running in fleet mode) to find the shared
/// sealing key instead of the per-node deterministic SRK.
pub fn load_fleet_key(context: &mut Context) -> Result<KeyHandle, FleetError> {
    let persistent_handle = PersistentTpmHandle::new(FLEET_KEY_PERSISTENT_HANDLE)?;
    let tpm_handle = TpmHandle::Persistent(persistent_handle);
    let object_handle =
        context.execute_with_nullauth_session(|ctx| ctx.tr_from_tpm_public(tpm_handle))?;
    key_handle_from(object_handle)
}

/// Narrow a generic `ObjectHandle` down to `KeyHandle`. Confirmed via a real
/// compile (2026-09-06, see CHANGELOG) that `tss-esapi` 7.7.0 provides an
/// infallible `From<ObjectHandle> for KeyHandle`, not `TryFrom` — this
/// module's original doc comment flagged that uncertainty before this
/// project could ever actually compile against the crate.
fn key_handle_from(handle: ObjectHandle) -> Result<KeyHandle, FleetError> {
    Ok(KeyHandle::from(handle))
}

#[cfg(test)]
#[path = "fleet_tests.rs"]
mod fleet_tests;
