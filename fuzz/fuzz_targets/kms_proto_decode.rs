// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0
//
// Fuzz the prost-generated KMS v2 request decoders (ADR-0003). Request bodies
// are deserialized from gRPC frames sent by kube-apiserver; decoding must
// never panic on arbitrary bytes.

#![no_main]

use libfuzzer_sys::fuzz_target;
use prost::Message as _;
use sceau::kms::pb::{DecryptRequest, EncryptRequest, StatusRequest};

fuzz_target!(|data: &[u8]| {
    let _ = EncryptRequest::decode(data);
    let _ = DecryptRequest::decode(data);
    let _ = StatusRequest::decode(data);
});
