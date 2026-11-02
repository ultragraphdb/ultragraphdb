/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_transport_tls::{TlsError, TlsErrorEnum};

#[test]
fn test_constructors_build_their_variant() {
    assert_eq!(
        TlsError::TrustStoreUnreadable("/p".into(), "gone".into()).0,
        TlsErrorEnum::TrustStoreUnreadable {
            path: "/p".into(),
            reason: "gone".into()
        }
    );
    assert_eq!(
        TlsError::TrustStoreEmpty("/p".into()).0,
        TlsErrorEnum::TrustStoreEmpty("/p".into())
    );
    assert_eq!(
        TlsError::InvalidServerName("h".into()).0,
        TlsErrorEnum::InvalidServerName("h".into())
    );
    assert_eq!(
        TlsError::HandshakeFailed("h".into(), "bad".into()).0,
        TlsErrorEnum::HandshakeFailed {
            host: "h".into(),
            reason: "bad".into()
        }
    );
}
