/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use rustls::ProtocolVersion;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transport_tls::{TestPki, TlsTrust};

#[test]
fn test_each_pki_is_one_ca_certificate_and_a_fresh_one() -> TestResult {
    let pki = TestPki::new()?;
    assert!(pki.ca_pem().starts_with("-----BEGIN CERTIFICATE-----"));
    assert!(
        pki.ca_pem()
            .trim_end()
            .ends_with("-----END CERTIFICATE-----")
    );
    let trust =
        TlsTrust::from_pem("test CA", pki.ca_pem().as_bytes()).context("one certificate")?;
    assert_eq!(trust.len(), 1);
    // Two runs share no key, so their certificates differ.
    assert_ne!(pki.ca_pem(), TestPki::new()?.ca_pem());
    Ok(())
}

#[test]
fn test_a_pki_keeps_its_server_certificate_and_keys_in_pem() -> TestResult {
    let pki = TestPki::new()?;
    assert!(
        pki.server_chain_pem()
            .starts_with("-----BEGIN CERTIFICATE-----")
    );
    assert!(
        pki.server_key_pem()
            .starts_with("-----BEGIN PRIVATE KEY-----")
    );
    assert!(
        pki.other_key_pem()
            .starts_with("-----BEGIN PRIVATE KEY-----")
    );
    assert_ne!(pki.server_key_pem(), pki.other_key_pem());
    assert_ne!(pki.server_chain_pem(), pki.ca_pem());
    assert_eq!(pki.connector()?.trust().len(), 1);
    Ok(())
}

#[test]
fn test_a_tls_1_2_client_session_agrees_on_tls_1_2_with_a_peer_speaking_both() -> TestResult {
    use std::io::{Read, Write};
    let pki = TestPki::new()?;
    let mut client = pki.client_tls12("localhost")?;
    assert_eq!(client.host(), "localhost");
    let mut server = pki.peer(ultragraphdb_transport_tls::TestEcho::default())?;
    let mut buf = vec![0u8; 1 << 15];
    for _ in 0..4 {
        let n = client.transmit(&mut buf);
        server
            .write_all(buf.get(..n).unwrap_or_default())
            .context("to the server")?;
        let answer = server.read(&mut buf).context("from the server")?;
        client.receive(buf.get(..answer).unwrap_or_default())?;
    }
    assert_eq!(client.protocol_version(), Some(ProtocolVersion::TLSv1_2));
    Ok(())
}
