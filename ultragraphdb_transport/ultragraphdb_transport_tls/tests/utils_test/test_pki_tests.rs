/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_transport_tls::{TestPki, TlsTrust};

#[test]
fn test_each_pki_is_one_ca_certificate_and_a_fresh_one() {
    let pki = TestPki::new();
    assert!(pki.ca_pem().starts_with("-----BEGIN CERTIFICATE-----"));
    assert!(
        pki.ca_pem()
            .trim_end()
            .ends_with("-----END CERTIFICATE-----")
    );
    let trust = TlsTrust::from_pem("test CA", pki.ca_pem().as_bytes()).expect("one certificate");
    assert_eq!(trust.len(), 1);
    // Two runs share no key, so their certificates differ.
    assert_ne!(pki.ca_pem(), TestPki::default().ca_pem());
}
