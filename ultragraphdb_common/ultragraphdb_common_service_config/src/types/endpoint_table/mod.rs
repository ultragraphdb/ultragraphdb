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
    held_bytes: Option<u64>,
    refusal_interval_ms: Option<u64>,
}

impl EndpointTable {
    /// `held_bytes` when the table names none: 4,194,304 bytes (`PROTOCOL.md` §11.9).
    pub const HELD_BYTES_DEFAULT: u64 = 4_194_304;

    pub(crate) fn read(reader: &mut SettingsReader) -> Result<Self, ServiceConfigError> {
        Ok(Self {
            address: reader.string("endpoint", "address")?,
            tls_cert_file: reader.string("endpoint", "tls_cert_file")?,
            tls_key_file: reader.string("endpoint", "tls_key_file")?,
            plaintext: reader.boolean("endpoint", "plaintext")?,
            connections_max: reader.u32("endpoint", "connections_max")?,
            held_bytes: reader.u64("endpoint", "held_bytes")?,
            refusal_interval_ms: reader.u64("endpoint", "refusal_interval_ms")?,
        })
    }

    /// `address`: the address clients connect to.
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

    /// `plaintext`: whether the endpoint speaks clear text.
    #[must_use]
    pub fn plaintext(&self) -> Option<bool> {
        self.plaintext
    }

    /// `connections_max`: the most connections the endpoint holds.
    #[must_use]
    pub fn connections_max(&self) -> Option<u32> {
        self.connections_max
    }

    /// `held_bytes`: the most bytes of Write and Operation frames a connection's reads hold back, and the bytes on its way to a role at
    /// which a connection stops being read.
    #[must_use]
    pub fn held_bytes(&self) -> Option<u64> {
        self.held_bytes
    }

    /// `refusal_interval_ms`: how long a principal past its failed authentications is refused.
    #[must_use]
    pub fn refusal_interval_ms(&self) -> Option<u64> {
        self.refusal_interval_ms
    }

    /// How the endpoint speaks. Clear text is only ever chosen by name, so an endpoint deciding nothing does not start.
    ///
    /// # Errors
    ///
    /// [`ServiceConfigErrorEnum::NoEndpointSecurity`] for neither a certificate nor `plaintext = true`;
    /// [`ServiceConfigErrorEnum::ConflictingEndpointSecurity`] for both;
    /// [`ServiceConfigErrorEnum::IncompleteEndpointSecurity`] for only one of the certificate and the key.
    pub fn security(&self) -> Result<EndpointSecurity, ServiceConfigError> {
        security_of(
            self.tls_cert_file.as_deref(),
            self.tls_key_file.as_deref(),
            self.plaintext,
        )
    }
}

/// How a listener of `cert_file`, `key_file` and `plaintext` speaks, by the endpoint's rule.
pub(crate) fn security_of(
    cert_file: Option<&str>,
    key_file: Option<&str>,
    plaintext: Option<bool>,
) -> Result<EndpointSecurity, ServiceConfigError> {
    let plaintext = plaintext == Some(true);
    match (cert_file, key_file) {
        (None, None) if plaintext => Ok(EndpointSecurity::Plaintext),
        (None, None) => Err(ServiceConfigError(
            ServiceConfigErrorEnum::NoEndpointSecurity,
        )),
        _ if plaintext => Err(ServiceConfigError(
            ServiceConfigErrorEnum::ConflictingEndpointSecurity,
        )),
        (Some(cert_file), Some(key_file)) => Ok(EndpointSecurity::Tls {
            cert_file: cert_file.to_string(),
            key_file: key_file.to_string(),
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
