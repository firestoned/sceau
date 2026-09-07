// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0
//! Integration test for the `TPM2_Duplicate` authorization fix (2026-09-06
//! -- see `.claude/CHANGELOG.md` and `fleet.rs`'s module doc comment): a
//! fresh HMAC session cannot authorize `TPM2_Duplicate` on the object being
//! duplicated (`TPM_RC_AUTH_TYPE`) -- only a policy session can. This
//! exercises the exact sequence that failed live: `create_fleet_key` ->
//! `create_transport_key` -> `duplicate_for_joiner`, against a real TPM or
//! software TPM. One TPM is sufficient for this specific regression --
//! the auth-session-type check this test targets is a local authorization
//! check against `fleet_key_handle`, independent of whether the "joiner"
//! in a real deployment is a genuinely separate physical TPM.
//!
//! Requires a real TPM or simulator (e.g. `swtpm`) -- `#[ignore]`d so
//! `cargo test` skips it by default. Point `SCEAU_TEST_TCTI` at one:
//!
//! ```sh
//! swtpm socket --tpm2 --tpmstate dir=/tmp/sceau-swtpm --flags not-need-init \
//!   --ctrl type=tcp,port=2322 --server type=tcp,port=2321 &
//! SCEAU_TEST_TCTI="swtpm:host=127.0.0.1,port=2321" \
//!   cargo test --test fleet_duplication -- --ignored --nocapture
//! ```
//!
//! **Unverified in this session.** This sandbox cannot compile `sceau` at
//! all (no macOS ARM `tss-esapi-sys` support, confirmed repeatedly this
//! session), so this test has never actually been run, and the exact
//! `swtpm` TCTI conf string syntax shown above was not independently
//! confirmed against a live `swtpm` invocation for the same reason --
//! `tss-esapi`'s `TctiNameConf::from_str` call itself mirrors the pattern
//! already used (and working, per the CHANGELOG) for `device:/dev/tpmrm0`.

use std::str::FromStr;

use sceau::fleet;
use tss_esapi::tcti_ldr::TctiNameConf;
use tss_esapi::Context;

fn test_context() -> Context {
    let tcti = std::env::var("SCEAU_TEST_TCTI").expect(
        "set SCEAU_TEST_TCTI to a TCTI conf string for a real TPM or swtpm, \
         e.g. SCEAU_TEST_TCTI=\"swtpm:host=127.0.0.1,port=2321\"",
    );
    let tcti = TctiNameConf::from_str(&tcti).expect("invalid SCEAU_TEST_TCTI");
    Context::new(tcti).expect("connecting to TPM")
}

#[test]
#[ignore = "requires a real TPM or swtpm -- see module doc comment"]
fn duplicate_for_joiner_succeeds_with_policy_session() {
    let mut context = test_context();

    let fleet_key_handle = fleet::create_fleet_key(&mut context, false)
        .expect("create_fleet_key must build a duplicable key with a matching authPolicy");

    let (_transport_handle, transport_public) =
        fleet::create_transport_key(&mut context).expect("create_transport_key");

    // This is the exact call that failed live with TPM_RC_AUTH_TYPE before
    // the policy-session fix: TPM2_Duplicate requires ADMIN-role
    // authorization on fleet_key_handle, which only a policy session can
    // provide -- a nullauth (HMAC) session was rejected outright.
    let blob = fleet::duplicate_for_joiner(&mut context, fleet_key_handle, transport_public)
        .expect("duplicate_for_joiner must succeed with a real policy session");

    assert!(blob.encryption_key.is_some());
}
