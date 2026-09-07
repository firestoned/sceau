// Copyright (c) 2026 Erick Bourgeois, sceau
// SPDX-License-Identifier: Apache-2.0
//! k0s-PKI-backed TLS identities for the ADR-0003 enrollment RPC.
//!
//! **Design note, recorded as an addendum to ADR-0003's Decision 3:** k0s's
//! kubelet-client cert (`/var/lib/k0s/kubelet/pki/kubelet-client-current.pem`)
//! has no Subject Alternative Name and `Extended Key Usage: TLS Web Client
//! Authentication` only (confirmed live, against a real k0s node's own
//! issued cert) -- it authenticates the *joiner* to the seed (Decision 3's
//! client-cert design), but cannot serve as the seed's own TLS server
//! identity: rustls requires SAN-based hostname verification with no CN
//! fallback. The seed instead mints a short-lived leaf certificate at
//! `enroll` startup, signed by k0s's own CA
//! (`/var/lib/k0s/pki/ca.crt` + `ca.key`, both already present on every
//! controller) -- same trust root, no new PKI, just a purpose-built leaf
//! the way apiserver/scheduler/ccm/etc. each already get their own.
//!
//! **Live-run-verified, one real bug found and fixed (2026-09-06):** a real
//! `enroll` run against a real k0s node's on-disk CA key failed with
//! `rcgen::Error::CouldNotParseKeyPair`. Root cause, confirmed against
//! `rcgen` 0.14's own docs: k0s writes its CA key as PKCS#1
//! (`-----BEGIN RSA PRIVATE KEY-----`), but `rcgen`'s default `ring` crypto
//! backend only parses PKCS#8 (`-----BEGIN PRIVATE KEY-----`) -- PKCS#1
//! support exists in `rcgen` only under the `aws_lc_rs` backend, which needs
//! `cmake`/`perl` to build (a new native-toolchain dependency not worth
//! taking after this session's QEMU/collect2 build fights). Fixed instead by
//! re-encoding losslessly to PKCS#8 in pure Rust via `rsa`'s
//! `pkcs1`/`pkcs8` support before handing the key to `rcgen` --
//! see `ca_key_to_pkcs8_pem` below.

use std::path::Path;

use rcgen::{CertificateParams, DnType, ExtendedKeyUsagePurpose, Issuer, KeyPair, KeyUsagePurpose};
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs8::{EncodePrivateKey, LineEnding};
use rsa::RsaPrivateKey;
use thiserror::Error;
use time::{Duration, OffsetDateTime};

use crate::authz::{node_name_from_cn, AuthzError};

const LEAF_CERT_VALIDITY_DAYS: i64 = 1;

/// k0s's default `--data-dir` -- everything this module reads
/// (`pki/ca.crt`, `pki/ca.key`, `kubelet/pki/kubelet-client-current.pem`)
/// lives under here, but k0s lets an operator relocate it, so every caller
/// threads its own `data_dir: &Path` through rather than assuming this.
pub const K0S_DEFAULT_DATA_DIR: &str = "/var/lib/k0s";

#[derive(Error, Debug)]
pub enum CertError {
    #[error("reading {0}: {1}")]
    Read(String, std::io::Error),
    #[error("parsing {0}: {1:?}")]
    Parse(String, String),
    #[error("{0} has no Subject Common Name")]
    NoCommonName(String),
    #[error("{0}")]
    Authz(#[from] AuthzError),
    #[error("minting certificate: {0}")]
    Rcgen(#[from] rcgen::Error),
    #[error("computing certificate validity window")]
    InvalidValidityWindow,
    #[error("converting CA key from PKCS#1 to PKCS#8: {0}")]
    Pkcs1(rsa::pkcs1::Error),
    #[error("encoding CA key as PKCS#8: {0}")]
    Pkcs8(rsa::pkcs8::Error),
}

/// This node's own k0s node identity (`system:node:<name>` CN, minus the
/// prefix) -- read from the same kubelet-client cert k0s already issued it,
/// not a separately detected hostname, so it's guaranteed to match whatever
/// DNS name an operator points `join --seed=` at.
pub fn own_node_name(data_dir: &Path) -> Result<String, CertError> {
    let path = data_dir.join("kubelet/pki/kubelet-client-current.pem");
    let pem_bytes =
        std::fs::read(&path).map_err(|e| CertError::Read(path.display().to_string(), e))?;
    let cn = common_name_from_pem(&pem_bytes, &path.display().to_string())?;
    Ok(node_name_from_cn(&cn)?.to_string())
}

/// Re-encode a PEM private key as PKCS#8 if it isn't already -- k0s writes
/// its CA key as PKCS#1 (`RSA PRIVATE KEY`), which `rcgen`'s `ring` backend
/// can't parse directly (see this module's doc comment). A no-op if `pem`
/// is already PKCS#8.
fn ca_key_to_pkcs8_pem(pem: &str) -> Result<String, CertError> {
    if pem.contains("BEGIN PRIVATE KEY") {
        return Ok(pem.to_string());
    }
    let key = RsaPrivateKey::from_pkcs1_pem(pem).map_err(CertError::Pkcs1)?;
    Ok(key
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(CertError::Pkcs8)?
        .to_string())
}

/// Mint a short-lived TLS server leaf for this node, signed by k0s's own CA,
/// with SAN = CN = `node_name`. In-memory only -- never written to disk,
/// since it only needs to live for this one `enroll` invocation. Returns
/// `(cert_pem, key_pem)`, ready for `tonic::transport::Identity::from_pem`.
pub fn mint_enroll_server_identity(
    node_name: &str,
    data_dir: &Path,
) -> Result<(String, String), CertError> {
    let ca_cert_path = data_dir.join("pki/ca.crt");
    let ca_key_path = data_dir.join("pki/ca.key");
    let ca_cert_pem = std::fs::read_to_string(&ca_cert_path)
        .map_err(|e| CertError::Read(ca_cert_path.display().to_string(), e))?;
    let ca_key_pem = std::fs::read_to_string(&ca_key_path)
        .map_err(|e| CertError::Read(ca_key_path.display().to_string(), e))?;
    let ca_key_pem = ca_key_to_pkcs8_pem(&ca_key_pem)?;
    let ca_key = KeyPair::from_pem(&ca_key_pem)?;
    let issuer = Issuer::from_ca_cert_pem(&ca_cert_pem, ca_key)?;

    let mut params = CertificateParams::new(vec![node_name.to_string()])?;
    params
        .distinguished_name
        .push(DnType::CommonName, node_name);
    params.use_authority_key_identifier_extension = true;
    params.key_usages.push(KeyUsagePurpose::DigitalSignature);
    params
        .extended_key_usages
        .push(ExtendedKeyUsagePurpose::ServerAuth);

    let validity = Duration::days(LEAF_CERT_VALIDITY_DAYS);
    let now = OffsetDateTime::now_utc();
    params.not_before = now
        .checked_sub(validity)
        .ok_or(CertError::InvalidValidityWindow)?;
    params.not_after = now
        .checked_add(validity)
        .ok_or(CertError::InvalidValidityWindow)?;

    let leaf_key = KeyPair::generate()?;
    let leaf_cert = params.signed_by(&leaf_key, &issuer)?;
    Ok((leaf_cert.pem(), leaf_key.serialize_pem()))
}

/// Extract the Subject CN from a DER-encoded peer certificate (as returned
/// by `tonic::Request::peer_certs()`), for the enroll server's Decision-3
/// authorization check.
pub fn peer_cert_common_name(der: &[u8]) -> Result<String, CertError> {
    let (_, cert) = x509_parser::parse_x509_certificate(der)
        .map_err(|e| CertError::Parse("peer certificate".into(), format!("{e:?}")))?;
    common_name_from_x509(&cert, "peer certificate")
}

fn common_name_from_pem(pem_bytes: &[u8], path: &str) -> Result<String, CertError> {
    let (_, pem) = x509_parser::pem::parse_x509_pem(pem_bytes)
        .map_err(|e| CertError::Parse(path.into(), format!("{e:?}")))?;
    let cert = pem
        .parse_x509()
        .map_err(|e| CertError::Parse(path.into(), format!("{e:?}")))?;
    common_name_from_x509(&cert, path)
}

fn common_name_from_x509(
    cert: &x509_parser::certificate::X509Certificate<'_>,
    label: &str,
) -> Result<String, CertError> {
    cert.subject()
        .iter_common_name()
        .next()
        .and_then(|attr| attr.as_str().ok())
        .map(str::to_string)
        .ok_or_else(|| CertError::NoCommonName(label.to_string()))
}

#[cfg(test)]
#[path = "certs_tests.rs"]
mod certs_tests;
