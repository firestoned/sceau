// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0
//! Unit tests for `tpm.rs`. Everything except `derive_key_id` and the
//! envelope encode/decode pair requires a live `Context`/TPM (create_primary,
//! create, load, unseal) — per `rules/testing.md`, those belong in
//! `swtpm`-gated integration tests under `tests/`, not here.
//!
//! The envelope-decode rejection cases below are the unit-test counterpart to
//! `fuzz/fuzz_targets/envelope_decode.rs`: the fuzzer proves the parser never
//! panics on arbitrary bytes, these pin the specific malformed shapes to a
//! `MalformedEnvelope` error.

#[cfg(test)]
mod tests {
    use super::super::*;

    /// Builds a sealed-object public area and an arbitrary private blob, then
    /// encodes them into an envelope.
    fn sample_envelope(private_bytes: &[u8]) -> Vec<u8> {
        let public = sealed_public(true).expect("sealed public template builds");
        let private = Private::try_from(private_bytes.to_vec()).expect("private blob is valid");
        envelope_encode(&public, &private)
    }

    #[test]
    fn derive_key_id_is_deterministic() {
        let name = b"some tpm object name bytes";
        assert_eq!(derive_key_id(name), derive_key_id(name));
    }

    #[test]
    fn derive_key_id_differs_for_different_names() {
        assert_ne!(derive_key_id(b"name-a"), derive_key_id(b"name-b"));
    }

    #[test]
    fn derive_key_id_has_sceau_prefix() {
        assert!(derive_key_id(b"whatever").starts_with("sceau-"));
    }

    #[test]
    fn sealed_public_fixed_tpm_and_fixed_parent_match_the_flag() {
        // Locks in the 2026-09-06 live fix: TPM2_Create rejects a
        // fixedTpm=true child under a fixedTpm=false (duplicable) parent
        // with TPM_RC_ATTRIBUTES -- sealed_public's argument must actually
        // control both bits, not just be accepted and ignored.
        let fixed = sealed_public(true).expect("valid template");
        assert!(fixed.object_attributes().fixed_tpm());
        assert!(fixed.object_attributes().fixed_parent());

        let duplicable = sealed_public(false).expect("valid template");
        assert!(!duplicable.object_attributes().fixed_tpm());
        assert!(!duplicable.object_attributes().fixed_parent());
    }

    #[test]
    fn envelope_round_trips_public_and_private() {
        // Exercises envelope_encode/envelope_decode without any TPM I/O --
        // Public/Private here are just marshalled/unmarshalled byte
        // buffers as far as this pair is concerned.
        let public = sealed_public(true).expect("sealed_public template is valid");
        let private = Private::try_from(vec![1u8, 2, 3, 4]).expect("small Private buffer");
        let envelope = envelope_encode(&public, &private);
        let (decoded_public, decoded_private) =
            envelope_decode(&envelope).expect("envelope round-trips");
        assert_eq!(decoded_public, public);
        assert_eq!(decoded_private.value(), private.value());
    }

    #[test]
    fn envelope_decode_rejects_empty_input() {
        assert!(matches!(
            envelope_decode(&[]),
            Err(TpmError::MalformedEnvelope)
        ));
    }

    #[test]
    fn envelope_decode_rejects_input_shorter_than_the_header() {
        // Two bytes: fewer than the version(1) + public_len(2) header, but
        // not empty -- a distinct path from the empty-input case above.
        let too_short = [ENVELOPE_VERSION, 0x00];
        assert!(matches!(
            envelope_decode(&too_short),
            Err(TpmError::MalformedEnvelope)
        ));
    }

    #[test]
    fn envelope_decode_rejects_wrong_version() {
        assert!(matches!(
            envelope_decode(&[ENVELOPE_VERSION.wrapping_add(1), 0, 0]),
            Err(TpmError::MalformedEnvelope)
        ));
    }

    #[test]
    fn envelope_decode_rejects_truncated_public() {
        // Claims a public_len far larger than the remaining bytes.
        assert!(matches!(
            envelope_decode(&[ENVELOPE_VERSION, 0xff, 0xff]),
            Err(TpmError::MalformedEnvelope)
        ));
    }

    #[test]
    fn envelope_decode_rejects_truncated_real_envelope() {
        // A genuine envelope with its tail lopped off: the length prefix is
        // plausible rather than absurd, so this reaches the bounds check by a
        // different route than the 0xffff case above.
        let envelope = sample_envelope(&[0x01]);
        let truncated = &envelope[..envelope.len() - 2];
        assert!(matches!(
            envelope_decode(truncated),
            Err(TpmError::MalformedEnvelope)
        ));
    }

    #[test]
    fn envelope_decode_rejects_garbage_public() {
        // Structurally valid header, but the public bytes are not a
        // marshalled TPMT_PUBLIC -- the unmarshal failure must surface as
        // MalformedEnvelope rather than propagating a Tss error.
        let mut garbage = vec![ENVELOPE_VERSION, 0x00, 0x10];
        garbage.extend_from_slice(&[0xFF; 16]);
        assert!(matches!(
            envelope_decode(&garbage),
            Err(TpmError::MalformedEnvelope)
        ));
    }
}
