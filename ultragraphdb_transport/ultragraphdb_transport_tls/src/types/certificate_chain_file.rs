/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The PEM file holding a server's certificate chain, leaf first: a path of its own type, so it cannot be passed where
/// a [`PrivateKeyFile`](crate::PrivateKeyFile) belongs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CertificateChainFile {
    path: String,
}

impl CertificateChainFile {
    /// The file at `path`.
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }

    /// Its path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
}
