/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::service_config_error::{ServiceConfigError, ServiceConfigErrorEnum};
use crate::types::endpoint_security::EndpointSecurity;
use crate::types::settings_reader::SettingsReader;

/// The `[endpoint]` table: the frontend's client endpoint.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EndpointTable {
    address: Option<String>,
    tls_cert_file: Option<String>,
    tls_key_file: Option<String>,
    plaintext: Option<bool>,
    connections_max: Option<u32>,
}

impl EndpointTable {
    pub(crate) fn read(reader: &mut SettingsReader) -> Result<Self, ServiceConfigError> {
        Ok(Self {
            address: reader.string("endpoint", "address")?,
            tls_cert_file: reader.string("endpoint", "tls_cert_file")?,
            tls_key_file: reader.string("endpoint", "tls_key_file")?,
            plaintext: reader.boolean("endpoint", "plaintext")?,
            connections_max: reader.u32("endpoint", "connections_max")?,
        })
    }

    /// `address`: the address clients connect to.
    pub fn address(&self) -> Option<&str> {
        self.address.as_deref()
    }

    /// `tls_cert_file`: the certificate chain's file.
    pub fn tls_cert_file(&self) -> Option<&str> {
        self.tls_cert_file.as_deref()
    }

    /// `tls_key_file`: the private key's file.
    pub fn tls_key_file(&self) -> Option<&str> {
        self.tls_key_file.as_deref()
    }

    /// `plaintext`: whether the endpoint speaks clear text.
    pub fn plaintext(&self) -> Option<bool> {
        self.plaintext
    }

    /// `connections_max`: the most connections the endpoint holds.
    pub fn connections_max(&self) -> Option<u32> {
        self.connections_max
    }

    /// How the endpoint speaks. Clear text is only ever chosen by name, so an endpoint deciding nothing does not start.
    ///
    /// # Errors
    ///
    /// [`ServiceConfigErrorEnum::NoEndpointSecurity`] for neither a certificate nor `plaintext = true`;
    /// [`ServiceConfigErrorEnum::ConflictingEndpointSecurity`] for both;
    /// [`ServiceConfigErrorEnum::IncompleteEndpointSecurity`] for only one of the certificate and the key.
    pub fn security(&self) -> Result<EndpointSecurity, ServiceConfigError> {
        let plaintext = self.plaintext == Some(true);
        match (&self.tls_cert_file, &self.tls_key_file) {
            (None, None) if plaintext => Ok(EndpointSecurity::Plaintext),
            (None, None) => Err(ServiceConfigError(
                ServiceConfigErrorEnum::NoEndpointSecurity,
            )),
            _ if plaintext => Err(ServiceConfigError(
                ServiceConfigErrorEnum::ConflictingEndpointSecurity,
            )),
            (Some(cert_file), Some(key_file)) => Ok(EndpointSecurity::Tls {
                cert_file: cert_file.clone(),
                key_file: key_file.clone(),
            }),
            (Some(_), None) => Err(ServiceConfigError(
                ServiceConfigErrorEnum::IncompleteEndpointSecurity {
                    absent: "tls_key_file",
                },
            )),
            (None, Some(_)) => Err(ServiceConfigError(
                ServiceConfigErrorEnum::IncompleteEndpointSecurity {
                    absent: "tls_cert_file",
                },
            )),
        }
    }
}
