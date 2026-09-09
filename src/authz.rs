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
    #[error("{0:?} was not named by --allow-node on this enroll invocation")]
    NotAllowed(String),
}

/// Check the joiner against the operator's `--allow-node` list.
///
/// Cluster membership alone is *not* a sufficient bar to receive the fleet
/// key: every kubelet in the cluster — including every worker, the
/// least-trusted machine class present — holds a certificate this CA signed
/// and has a corresponding `Node` object. Since the fleet key unseals every
/// DEK in the cluster, the operator names the specific joiner when starting
/// `enroll` rather than accepting any current member.
///
/// A node-role label check was considered instead and rejected: k0s
/// controllers do not necessarily carry `node-role.kubernetes.io/control-plane`,
/// and a k0s controller without `--enable-worker` has no `Node` object at all,
/// so keying on the label would reject legitimate joiners in some supported
/// topologies. An explicit list depends on no labelling convention.
///
/// # Arguments
/// * `node_name` - node identity taken from the peer certificate's CN
/// * `allowed` - names the operator passed as `--allow-node`
///
/// # Errors
/// Returns [`AuthzError::NotAllowed`] if `node_name` is not an exact match for
/// an entry in `allowed`. An empty `allowed` denies everything: this fails
/// closed, so a construction path that forgets to populate it cannot silently
/// become allow-all.
pub fn authorize_allowlist(node_name: &str, allowed: &[String]) -> Result<(), AuthzError> {
    if !allowed.iter().any(|a| a == node_name) {
        return Err(AuthzError::NotAllowed(node_name.to_string()));
    }
    Ok(())
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
