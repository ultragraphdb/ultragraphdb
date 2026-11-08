/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::service_config_error::ServiceConfigError;
use crate::types::endpoint_security::EndpointSecurity;
use crate::types::endpoint_table::security_of;
use crate::types::settings_reader::SettingsReader;

/// The `[admin]` table: a role's listener for what an administrator asks of it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AdminTable {
    address: Option<String>,
    tls_cert_file: Option<String>,
    tls_key_file: Option<String>,
    plaintext: Option<bool>,
    key_sha256: Option<String>,
}

impl AdminTable {
    pub(crate) fn read(reader: &mut SettingsReader) -> Result<Self, ServiceConfigError> {
        Ok(Self {
            address: reader.string("admin", "address")?,
            tls_cert_file: reader.string("admin", "tls_cert_file")?,
            tls_key_file: reader.string("admin", "tls_key_file")?,
            plaintext: reader.boolean("admin", "plaintext")?,
            key_sha256: reader.string("admin", "key_sha256")?,
        })
    }

    /// `address`: where the listener is.
    #[must_use]
    pub fn address(&self) -> Option<&str> {
        self.address.as_deref()
    }

    /// `tls_cert_file`: the certificate chain's file.
    #[must_use]
    pub fn tls_cert_file(&self) -> Option<&str> {
        self.tls_cert_file.as_deref()
    }

    /// `tls_key_file`: the private key's file.
    #[must_use]
    pub fn tls_key_file(&self) -> Option<&str> {
        self.tls_key_file.as_deref()
    }

    /// `key_sha256`: the SHA-256 digest of the admin key, 64 hexadecimal digits.
    #[must_use]
    pub fn key_sha256(&self) -> Option<&str> {
        self.key_sha256.as_deref()
    }

    /// How the listener speaks, by the endpoint's rule.
    ///
    /// # Errors
    ///
    /// As [`crate::EndpointTable::security`].
    pub fn security(&self) -> Result<EndpointSecurity, ServiceConfigError> {
        security_of(
            self.tls_cert_file.as_deref(),
            self.tls_key_file.as_deref(),
            self.plaintext,
        )
    }
}
