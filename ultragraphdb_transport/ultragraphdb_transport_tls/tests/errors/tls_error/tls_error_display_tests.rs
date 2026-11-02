/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_transport_tls::{TlsError, TlsErrorEnum};

#[test]
fn test_display_names_the_input_and_the_reason() {
    assert_eq!(
        TlsError::TrustStoreUnreadable("/p".into(), "gone".into()).to_string(),
        "cannot read the trust store \"/p\": gone"
    );
    assert_eq!(
        TlsError::TrustStoreEmpty("/p".into()).to_string(),
        "no usable certificate in /p"
    );
    assert_eq!(
        TlsError::InvalidServerName("a b".into()).to_string(),
        "no certificate can be checked against \"a b\""
    );
    assert_eq!(
        TlsError::HandshakeFailed("h".into(), "bad".into()).to_string(),
        "TLS handshake with \"h\" failed: bad"
    );
    assert_eq!(
        TlsError(TlsErrorEnum::ConfigurationRefused("no versions".into())).to_string(),
        "TLS configuration refused: no versions"
    );
}
