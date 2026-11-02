/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ultragraphdb_protocol_scram::{NONCE_BYTES, ScramClient, system_nonce};
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_a_system_nonce_is_its_random_bytes_in_base64_and_a_valid_nonce() -> TestResult {
    let nonce = system_nonce()?;
    assert_eq!(nonce.len(), 24);
    assert_eq!(STANDARD.decode(&nonce)?.len(), NONCE_BYTES);
    assert!(!nonce.contains(','));
    assert!(ScramClient::new("user", "pencil", &nonce).is_ok());
    Ok(())
}

#[test]
fn test_system_nonces_differ() -> TestResult {
    let nonces = (0..64)
        .map(|_| system_nonce())
        .collect::<Result<std::collections::BTreeSet<String>, _>>()?;
    assert_eq!(nonces.len(), 64);
    Ok(())
}

#[test]
fn test_every_byte_of_a_system_nonce_is_drawn() -> TestResult {
    let drawn = (0..64)
        .map(|_| Ok(STANDARD.decode(system_nonce()?)?))
        .collect::<TestResult<Vec<Vec<u8>>>>()?;
    for at in 0..NONCE_BYTES {
        assert!(
            drawn.iter().any(|bytes| bytes[at] != drawn[0][at]),
            "byte {at} never varies"
        );
    }
    Ok(())
}
