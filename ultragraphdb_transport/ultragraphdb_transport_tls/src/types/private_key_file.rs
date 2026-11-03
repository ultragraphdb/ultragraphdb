/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The PEM file holding a server certificate's private key: a path of its own type, so it cannot be passed where a
/// [`CertificateChainFile`](crate::CertificateChainFile) belongs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PrivateKeyFile {
    path: String,
}

impl PrivateKeyFile {
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
