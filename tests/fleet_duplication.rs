// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0
//! Integration tests for `TPM2_Duplicate` on the enrollment seed, against a
//! real TPM or simulator.
//!
//! Two properties are covered:
//!
//! 1. The 2026-09-06 authorization fix (see `.claude/CHANGELOG.md` and
//!    `fleet.rs`'s module doc comment): a fresh HMAC session cannot authorize
//!    `TPM2_Duplicate` on the object being duplicated (`TPM_RC_AUTH_TYPE`) --
//!    only a policy session can.
//! 2. `validate_transport_key` actually rejects an unacceptable duplication
//!    target *before* anything is wrapped to it.
//!
//! One TPM is sufficient: both checks are local authorization checks on the
//! seed side, independent of whether the "joiner" is a separate physical TPM
//! in a real deployment.
//!
//! Requires a real TPM or simulator (e.g. `swtpm`) -- `#[ignore]`d so
//! `cargo test` skips it by default. Point `SCEAU_TEST_TCTI` at one:
//!
//! ```sh
//! swtpm socket --tpm2 --tpmstate dir=/tmp/sceau-swtpm \
//!   --flags not-need-init,startup-clear \
//!   --ctrl type=tcp,port=2322 --server type=tcp,port=2321 &
//! SCEAU_TEST_TCTI="swtpm:host=127.0.0.1,port=2321" \
//!   cargo test --test fleet_duplication -- --ignored --nocapture --test-threads=1
//! ```
//!
//! `--test-threads=1` is required, not cosmetic: a TPM is a single shared
//! resource and both tests persist a fleet key at the same well-known handle.
//! Run in parallel they race, and the loser gets `TPM_RC_NV_DEFINED` (0x14C)
//! from `TPM2_EvictControl` on an already-occupied handle -- which is the
//! documented, correct behaviour of that command (it fails loudly rather than
//! silently overwriting), not a defect in the code under test.
//!
//! **Verified 2026-09-09** against `swtpm` 0.7.1 in a Linux container, which
//! is the first time this file has ever been executed. Running it immediately
//! found a real defect in the test itself: it held the joiner's transport-key
//! handle live across `duplicate_for_joiner`, so the fleet key, the transport
//! key and the loaded external object needed three transient object slots at
//! once and the TPM returned `TPM_RC_OBJECT_MEMORY` (0x902). A seed never
//! holds the joiner's handle in production -- it only ever receives a `Public`
//! over the wire -- so the fix is to flush it here, which is also what makes
//! the test model the real deployment.

use std::str::FromStr;

use sceau::fleet;
use tss_esapi::attributes::ObjectAttributesBuilder;
use tss_esapi::interface_types::algorithm::{HashingAlgorithm, PublicAlgorithm};
use tss_esapi::interface_types::key_bits::RsaKeyBits;
use tss_esapi::structures::{
    Public, PublicBuilder, PublicKeyRsa, PublicRsaParametersBuilder, RsaExponent,
    SymmetricDefinitionObject,
};
use tss_esapi::tcti_ldr::TctiNameConf;
use tss_esapi::Context;

fn test_context() -> Context {
    let tcti = std::env::var("SCEAU_TEST_TCTI").expect(
        "set SCEAU_TEST_TCTI to a TCTI conf string for a real TPM or swtpm, \
         e.g. SCEAU_TEST_TCTI=\"swtpm:host=127.0.0.1,port=2321\"",
    );
    let tcti = TctiNameConf::from_str(&tcti).expect("parsing SCEAU_TEST_TCTI");
    Context::new(tcti).expect("connecting to TPM")
}

/// A duplicable storage key -- exactly what the transport-key template used to
/// produce before it was made `fixedTpm`/`fixedParent`, and what a joiner
/// running older code (or an attacker) would submit.
fn duplicable_transport_public() -> Public {
    let attributes = ObjectAttributesBuilder::new()
        .with_sensitive_data_origin(true)
        .with_user_with_auth(true)
        .with_decrypt(true)
        .with_restricted(true)
        .build()
        .expect("attributes");
    PublicBuilder::new()
        .with_public_algorithm(PublicAlgorithm::Rsa)
        .with_name_hashing_algorithm(HashingAlgorithm::Sha256)
        .with_object_attributes(attributes)
        .with_rsa_parameters(
            PublicRsaParametersBuilder::new_restricted_decryption_key(
                SymmetricDefinitionObject::AES_128_CFB,
                RsaKeyBits::Rsa2048,
                RsaExponent::default(),
            )
            .build()
            .expect("rsa parameters"),
        )
        .with_rsa_unique_identifier(PublicKeyRsa::default())
        .build()
        .expect("public")
}

#[test]
#[ignore = "requires a real TPM or swtpm -- see module doc comment"]
fn duplicate_for_joiner_succeeds_with_policy_session() {
    let mut context = test_context();

    let fleet_key_handle = fleet::create_fleet_key(&mut context, false)
        .expect("create_fleet_key must build a duplicable key with a matching authPolicy");

    let (transport_handle, transport_public) =
        fleet::create_transport_key(&mut context).expect("create_transport_key");

    // A seed only ever receives the joiner's `Public` over the wire; it never
    // holds the joiner's handle. Flushing here models that, and keeps the
    // transient-object count within what a TPM actually offers -- holding it
    // live needs three slots at once and fails with TPM_RC_OBJECT_MEMORY.
    context
        .flush_context(transport_handle.into())
        .expect("flushing the joiner-side handle");

    // This is the exact call that failed live with TPM_RC_AUTH_TYPE before
    // the policy-session fix: TPM2_Duplicate requires ADMIN-role
    // authorization on fleet_key_handle, which only a policy session can
    // provide -- a nullauth (HMAC) session was rejected outright.
    let blob = fleet::duplicate_for_joiner(&mut context, fleet_key_handle, transport_public)
        .expect("duplicate_for_joiner must succeed with a real policy session");

    assert!(blob.encryption_key.is_some());
}

#[test]
#[ignore = "requires a real TPM or swtpm -- see module doc comment"]
fn duplicate_for_joiner_rejects_a_duplicable_transport_key() {
    let mut context = test_context();

    let fleet_key_handle = fleet::create_fleet_key(&mut context, false).expect("create_fleet_key");

    // Nothing may be wrapped to a target that can itself be duplicated away.
    // This must fail before TPM2_Duplicate is reached at all.
    // `DuplicationBlob` deliberately does not derive Debug (it carries wrapped
    // key material), so match rather than `expect_err`.
    let result = fleet::duplicate_for_joiner(
        &mut context,
        fleet_key_handle,
        duplicable_transport_public(),
    );
    let Err(err) = result else {
        panic!("a duplicable transport key must be rejected");
    };

    let msg = format!("{err}");
    assert!(
        msg.contains("fixedTpm"),
        "error should name the failing attribute, got: {msg}"
    );
}
