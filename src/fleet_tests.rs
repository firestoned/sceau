// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0
//! Unit tests for `fleet.rs`.
//!
//! Most of `fleet.rs` requires a live `Context`/TPM — `duplicable_storage_public`
//! runs a real `Trial` session to compute the object's `authPolicy`, so it
//! cannot be exercised here. Per `rules/testing.md`, TPM-dependent tests
//! belong in `swtpm`-gated integration tests under `tests/`.
//!
//! `transport_key_public` and `validate_transport_key` are the exceptions:
//! both are pure `Public`/attribute construction and inspection, with no
//! `Context`, precisely so the seed's validation rules can be pinned here.

#[cfg(test)]
mod tests {
    use super::super::*;

    /// A transport-key `Public` with individually overridable attributes, so
    /// each test can break exactly one rule and assert it is caught.
    #[allow(clippy::too_many_arguments)]
    fn transport_public_with(
        fixed_tpm: bool,
        fixed_parent: bool,
        restricted: bool,
        decrypt: bool,
        sign: bool,
        alg: PublicAlgorithm,
        name_alg: HashingAlgorithm,
        bits: RsaKeyBits,
    ) -> Public {
        let attributes = ObjectAttributesBuilder::new()
            .with_sensitive_data_origin(true)
            .with_user_with_auth(true)
            .with_decrypt(decrypt)
            .with_sign_encrypt(sign)
            .with_restricted(restricted)
            .with_fixed_tpm(fixed_tpm)
            .with_fixed_parent(fixed_parent)
            .build()
            .expect("attributes build");
        PublicBuilder::new()
            .with_public_algorithm(alg)
            .with_name_hashing_algorithm(name_alg)
            .with_object_attributes(attributes)
            .with_rsa_parameters(
                PublicRsaParametersBuilder::new_restricted_decryption_key(
                    duplication_symmetric_alg(),
                    bits,
                    RsaExponent::default(),
                )
                .build()
                .expect("rsa params build"),
            )
            .with_rsa_unique_identifier(Default::default())
            .build()
            .expect("public build")
    }

    fn good() -> Public {
        transport_public_with(
            true,
            true,
            true,
            true,
            false,
            PublicAlgorithm::Rsa,
            HashingAlgorithm::Sha256,
            RsaKeyBits::Rsa2048,
        )
    }

    #[test]
    fn transport_key_public_is_a_non_duplicable_restricted_storage_key() {
        // The template the joiner uses must itself satisfy the seed's rules,
        // or a legitimate join can never succeed.
        let public = transport_key_public().expect("template builds");
        let attrs = public.object_attributes();
        assert!(attrs.fixed_tpm(), "transport key must not be duplicable");
        assert!(
            attrs.fixed_parent(),
            "transport key must not be re-parented"
        );
        assert!(attrs.restricted());
        assert!(attrs.decrypt());
        assert!(!attrs.sign_encrypt());
    }

    #[test]
    fn transport_key_public_satisfies_validate_transport_key() {
        let public = transport_key_public().expect("template builds");
        validate_transport_key(&public).expect("own template must validate");
    }

    #[test]
    fn validate_transport_key_accepts_a_well_formed_key() {
        validate_transport_key(&good()).expect("well-formed key must validate");
    }

    #[test]
    fn validate_transport_key_rejects_a_duplicable_key() {
        // The point of the check: a transport key that can itself be
        // duplicated away offers no containment for the fleet key imported
        // under it.
        let public = transport_public_with(
            false,
            true,
            true,
            true,
            false,
            PublicAlgorithm::Rsa,
            HashingAlgorithm::Sha256,
            RsaKeyBits::Rsa2048,
        );
        assert!(validate_transport_key(&public).is_err());
    }

    #[test]
    fn validate_transport_key_rejects_a_re_parentable_key() {
        let public = transport_public_with(
            true,
            false,
            true,
            true,
            false,
            PublicAlgorithm::Rsa,
            HashingAlgorithm::Sha256,
            RsaKeyBits::Rsa2048,
        );
        assert!(validate_transport_key(&public).is_err());
    }

    #[test]
    fn validate_transport_key_rejects_an_unrestricted_key() {
        let public = transport_public_with(
            true,
            true,
            false,
            true,
            false,
            PublicAlgorithm::Rsa,
            HashingAlgorithm::Sha256,
            RsaKeyBits::Rsa2048,
        );
        assert!(validate_transport_key(&public).is_err());
    }

    #[test]
    fn validate_transport_key_rejects_a_non_decryption_key() {
        let public = transport_public_with(
            true,
            true,
            true,
            false,
            false,
            PublicAlgorithm::Rsa,
            HashingAlgorithm::Sha256,
            RsaKeyBits::Rsa2048,
        );
        assert!(validate_transport_key(&public).is_err());
    }

    #[test]
    fn validate_transport_key_rejects_a_signing_key() {
        let public = transport_public_with(
            true,
            true,
            true,
            true,
            true,
            PublicAlgorithm::Rsa,
            HashingAlgorithm::Sha256,
            RsaKeyBits::Rsa2048,
        );
        assert!(validate_transport_key(&public).is_err());
    }

    #[test]
    fn validate_transport_key_rejects_a_weaker_name_algorithm() {
        // The Name is the hash of the public area under nameAlg. Accepting a
        // weaker hash here would weaken the binding that ADR-0003 Phase 6's
        // credential-activation challenge will rest on.
        let public = transport_public_with(
            true,
            true,
            true,
            true,
            false,
            PublicAlgorithm::Rsa,
            HashingAlgorithm::Sha1,
            RsaKeyBits::Rsa2048,
        );
        assert!(validate_transport_key(&public).is_err());
    }

    #[test]
    fn validate_transport_key_rejects_an_undersized_rsa_key() {
        let public = transport_public_with(
            true,
            true,
            true,
            true,
            false,
            PublicAlgorithm::Rsa,
            HashingAlgorithm::Sha256,
            RsaKeyBits::Rsa1024,
        );
        assert!(validate_transport_key(&public).is_err());
    }

    #[test]
    fn validate_transport_key_error_is_specific_about_what_failed() {
        let public = transport_public_with(
            false,
            true,
            true,
            true,
            false,
            PublicAlgorithm::Rsa,
            HashingAlgorithm::Sha256,
            RsaKeyBits::Rsa2048,
        );
        let err = validate_transport_key(&public).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("fixedTpm"),
            "error should name the failing attribute, got: {msg}"
        );
    }
}
