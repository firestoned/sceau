// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(false)
        .compile_protos(&["proto/kms/v2/api.proto"], &["proto"])?;
    // Enrollment RPC (ADR-0003) needs both: the seed serves it, the joiner
    // calls it -- unlike the KMS v2 socket, which kube-apiserver always
    // calls into (sceau is never a KMS v2 *client*).
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/enroll/v1/api.proto"], &["proto"])?;
    Ok(())
}
