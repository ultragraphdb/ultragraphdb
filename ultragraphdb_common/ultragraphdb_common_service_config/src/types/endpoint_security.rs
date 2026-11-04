/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// How the client endpoint speaks, once its settings have decided it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointSecurity {
    /// TLS 1.3 from the first byte.
    Tls {
        /// The certificate chain's file.
        cert_file: String,
        /// The private key's file.
        key_file: String,
    },
    /// Clear text, which the endpoint's settings asked for by name.
    Plaintext,
}
