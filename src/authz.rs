// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0
//! ADR-0003 Decision 3: enrollment authorization is a check separate from
//! mTLS authentication. mTLS (see `certs.rs`) proves the connecting peer
//! holds a private key matching a certificate k0s's own CA issued; this
//! module proves that identity is *currently a real member of this
//! cluster's own `Node` objects* -- the actual bar to request the fleet
//! key, not merely "holds any k0s-signed cert."
//!
//! **Unverified in this session** beyond the CN format itself: `kube`'s
//! `Api::get` call was checked against the crate's documented signature,
//! not compiled -- this sandbox cannot build `sceau` at all (no macOS ARM
//! `tss-esapi-sys` support). The CN format
//! (`O=system:nodes, CN=system:node:<hostname>`) *was* confirmed live,
//! against a real k0s-issued kubelet-client cert on a test node.

use k8s_openapi::api::core::v1::Node;
use kube::Api;
use thiserror::Error;

/// k8s's own kubelet-client CN convention -- the standard Node-authorization
/// identity format, not something sceau invented.
const NODE_IDENTITY_PREFIX: &str = "system:node:";

#[derive(Error, Debug)]
pub enum AuthzError {
    #[error(
        "client certificate CN {0:?} is not a k0s node identity (expected \"system:node:<name>\")"
    )]
    NotNodeIdentity(String),
    #[error("{0:?} is not a current member of this cluster's Node objects")]
    UnknownNode(String),
}

/// Extract the node name from a kubelet client cert's Subject CN.
pub fn node_name_from_cn(cn: &str) -> Result<&str, AuthzError> {
    let Some(name) = cn.strip_prefix(NODE_IDENTITY_PREFIX) else {
        return Err(AuthzError::NotNodeIdentity(cn.to_string()));
    };
    if name.is_empty() {
        return Err(AuthzError::NotNodeIdentity(cn.to_string()));
    }
    Ok(name)
}

/// Authorize an enrollment request: the presented identity must name a
/// `Node` object that actually exists in this cluster right now.
pub async fn authorize_node(client: &kube::Client, node_name: &str) -> Result<(), AuthzError> {
    Api::<Node>::all(client.clone())
        .get(node_name)
        .await
        .map_err(|_| AuthzError::UnknownNode(node_name.to_string()))?;
    Ok(())
}

#[cfg(test)]
#[path = "authz_tests.rs"]
mod authz_tests;
