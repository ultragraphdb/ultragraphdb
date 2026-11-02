/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::env;
use std::sync::Arc;

use rustls::RootCertStore;
use rustls_pki_types::CertificateDer;
use rustls_pki_types::pem::PemObject;

use crate::errors::tls_error::TlsError;

/// Where the image keeps the CA bundle it ships. A static binary has no system library to ask for trusted roots,
/// so the image copies this file in.
pub const DEFAULT_CA_BUNDLE: &str = "/etc/ssl/certs/ca-certificates.crt";

/// The variable naming another bundle, as OpenSSL, curl and Go honour it. A private CA is trusted by pointing it at
/// a bundle that holds the CA.
pub const CA_BUNDLE_KEY: &str = "SSL_CERT_FILE";

/// The certificates a connection is verified against: the anchors of one PEM source, each once. Every constructor
/// refuses a source with no usable certificate, because a store that trusts nothing fails every verification and
/// saying so once beats saying it per connection.
#[derive(Debug, Clone)]
pub struct TlsTrust {
    // Shared, not copied: every connection a connector opens verifies against this one store.
    roots: Arc<RootCertStore>,
    source: String,
}

impl TlsTrust {
    /// The image's bundle: the file `SSL_CERT_FILE` names, or [`DEFAULT_CA_BUNDLE`] when it is unset or empty.
    ///
    /// # Errors
    ///
    /// As [`Self::from_pem_file`].
    pub fn system() -> Result<Self, TlsError> {
        Self::system_from(env::var(CA_BUNDLE_KEY).ok().as_deref())
    }

    /// The bundle `ssl_cert_file` names, the value of `SSL_CERT_FILE` passed in rather than read, or
    /// [`DEFAULT_CA_BUNDLE`] when it is `None` or empty.
    ///
    /// # Errors
    ///
    /// As [`Self::from_pem_file`].
    pub fn system_from(ssl_cert_file: Option<&str>) -> Result<Self, TlsError> {
        let path = ssl_cert_file
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_CA_BUNDLE);
        Self::from_pem_file(path)
    }

    /// The certificates in the PEM file at `path`.
    ///
    /// # Errors
    ///
    /// [`TlsErrorEnum::TrustStoreUnreadable`](crate::TlsErrorEnum::TrustStoreUnreadable) when the file cannot be
    /// read, and [`TrustStoreEmpty`](crate::TlsErrorEnum::TrustStoreEmpty) when it holds no usable certificate.
    pub fn from_pem_file(path: &str) -> Result<Self, TlsError> {
        let pem = std::fs::read(path)
            .map_err(|e| TlsError::trust_store_unreadable(path.to_string(), e.to_string()))?;
        Self::from_pem(path, &pem)
    }

    /// The certificates in `pem`, named `source` in diagnostics. Certificates no verifier takes are skipped, and an
    /// anchor listed twice is kept once.
    ///
    /// A bundle routinely holds a few certificates no verifier takes, an old encoding or a retired algorithm, so
    /// those are skipped rather than refusing the rest; a duplicate is dropped because the store is searched
    /// linearly on every verification.
    ///
    /// # Errors
    ///
    /// [`TlsErrorEnum::TrustStoreEmpty`](crate::TlsErrorEnum::TrustStoreEmpty) when `pem` holds no usable
    /// certificate.
    pub fn from_pem(source: &str, pem: &[u8]) -> Result<Self, TlsError> {
        let certificates: Vec<CertificateDer<'static>> =
            CertificateDer::pem_slice_iter(pem).flatten().collect();
        let mut roots = RootCertStore::empty();
        let (added, ignored) = roots.add_parsable_certificates(certificates.iter().cloned());
        debug_assert_eq!(added + ignored, certificates.len());
        let anchors = std::mem::take(&mut roots.roots);
        for anchor in anchors {
            if !roots.roots.contains(&anchor) {
                roots.roots.push(anchor);
            }
        }
        debug_assert!(roots.len() <= added);
        if roots.is_empty() {
            return Err(TlsError::trust_store_empty(source.to_string()));
        }
        Ok(Self {
            roots: Arc::new(roots),
            source: source.to_string(),
        })
    }

    /// How many certificates are trusted.
    #[must_use]
    pub fn len(&self) -> usize {
        self.roots.len()
    }

    /// Whether nothing is trusted: always false, since every constructor refuses an empty store.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    /// Where the certificates came from, for diagnostics.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The store, for a client configuration.
    pub(crate) fn roots(&self) -> Arc<RootCertStore> {
        Arc::clone(&self.roots)
    }
}
