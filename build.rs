// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .compile_protos(&["proto/kms/v2/api.proto"], &["proto"])?;
    Ok(())
}
