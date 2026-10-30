/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ultragraphdb_protocol_scram::{NONCE_BYTES, ScramClient, system_nonce};

#[test]
fn test_a_system_nonce_is_its_random_bytes_in_base64_and_a_valid_nonce() {
    let nonce = system_nonce().unwrap();
    assert_eq!(nonce.len(), 24);
    assert_eq!(STANDARD.decode(&nonce).unwrap().len(), NONCE_BYTES);
    assert!(!nonce.contains(','));
    assert!(ScramClient::new("user", "pencil", &nonce).is_ok());
}

#[test]
fn test_system_nonces_differ() {
    let nonces: std::collections::BTreeSet<String> =
        (0..64).map(|_| system_nonce().unwrap()).collect();
    assert_eq!(nonces.len(), 64);
}

#[test]
fn test_every_byte_of_a_system_nonce_is_drawn() {
    let drawn: Vec<Vec<u8>> = (0..64)
        .map(|_| STANDARD.decode(system_nonce().unwrap()).unwrap())
        .collect();
    for at in 0..NONCE_BYTES {
        assert!(
            drawn.iter().any(|bytes| bytes[at] != drawn[0][at]),
            "byte {at} never varies"
        );
    }
}
