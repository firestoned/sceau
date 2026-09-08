// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0
//
// Fuzz the KMS ciphertext envelope parser (ADR-0003). The envelope bytes
// arrive from kube-apiserver, so this parser must never panic and must
// reject malformed input with TpmError::MalformedEnvelope.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = sceau::tpm::envelope_decode(data);
});
