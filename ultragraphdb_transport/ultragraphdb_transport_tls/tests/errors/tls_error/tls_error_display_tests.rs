/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_transport_tls::{TlsError, TlsErrorEnum};

#[test]
fn test_display_names_the_input_and_the_reason() {
    assert_eq!(
        TlsError::trust_store_unreadable("/p".into(), "gone".into()).to_string(),
        "cannot read the trust store \"/p\": gone"
    );
    assert_eq!(
        TlsError::trust_store_empty("/p".into()).to_string(),
        "no usable certificate in /p"
    );
    assert_eq!(
        TlsError::invalid_server_name("a b".into()).to_string(),
        "no certificate can be checked against \"a b\""
    );
    assert_eq!(
        TlsError::handshake_failed("h".into(), "bad".into()).to_string(),
        "TLS handshake with \"h\" failed: bad"
    );
    assert_eq!(
        TlsError(TlsErrorEnum::ConfigurationRefused("no versions".into())).to_string(),
        "TLS configuration refused: no versions"
    );
    assert_eq!(
        TlsError::certificate_unreadable("/c".into(), "gone".into()).to_string(),
        "cannot read the server certificate file \"/c\": gone"
    );
    assert_eq!(
        TlsError::certificate_empty("/c".into()).to_string(),
        "no certificate or private key in /c"
    );
    assert_eq!(
        TlsError::key_mismatch("/k".into()).to_string(),
        "the private key in /k does not match the chain's first certificate"
    );
    assert_eq!(
        TlsError::record_refused("h".into(), "bad".into()).to_string(),
        "TLS session with \"h\" failed: bad"
    );
}
