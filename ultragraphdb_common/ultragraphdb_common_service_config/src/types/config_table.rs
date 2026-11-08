/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::service_config_error::ServiceConfigError;
use crate::types::endpoint_security::EndpointSecurity;
use crate::types::endpoint_table::security_of;
use crate::types::settings_reader::SettingsReader;

/// The `[config]` table: the config service's own settings, the one file it reads locally.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigTable {
    address: Option<String>,
    tls_cert_file: Option<String>,
    tls_key_file: Option<String>,
    plaintext: Option<bool>,
    data_dir: Option<String>,
    read_key_file: Option<String>,
    admin_key_file: Option<String>,
    targets_file: Option<String>,
}

impl ConfigTable {
    pub(crate) fn read(reader: &mut SettingsReader) -> Result<Self, ServiceConfigError> {
        Ok(Self {
            address: reader.string("config", "address")?,
            tls_cert_file: reader.string("config", "tls_cert_file")?,
            tls_key_file: reader.string("config", "tls_key_file")?,
            plaintext: reader.boolean("config", "plaintext")?,
            data_dir: reader.string("config", "data_dir")?,
            read_key_file: reader.string("config", "read_key_file")?,
            admin_key_file: reader.string("config", "admin_key_file")?,
            targets_file: reader.string("config", "targets_file")?,
        })
    }

    /// `address`: where the service listens.
    #[must_use]
    pub fn address(&self) -> Option<&str> {
        self.address.as_deref()
    }

    /// `data_dir`: the directory holding its log.
    #[must_use]
    pub fn data_dir(&self) -> Option<&str> {
        self.data_dir.as_deref()
    }

    /// `read_key_file`: the file holding the read key.
    #[must_use]
    pub fn read_key_file(&self) -> Option<&str> {
        self.read_key_file.as_deref()
    }

    /// `admin_key_file`: the file holding the admin key.
    #[must_use]
    pub fn admin_key_file(&self) -> Option<&str> {
        self.admin_key_file.as_deref()
    }

    /// `targets_file`: the central file declaring every settings file.
    #[must_use]
    pub fn targets_file(&self) -> Option<&str> {
        self.targets_file.as_deref()
    }

    /// How the service speaks, by the endpoint's rule.
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
