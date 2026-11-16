/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_scram::{ScramServer, ScramVerifier};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::hidden::hides;
use crate::utils::rfc7677::{CLIENT_FIRST, ITERATIONS, NAME, SERVER_NONCE, verifier};

const SECRET: [u8; 32] = *b"the server's secret of 32 bytes.";

/// A server's `Debug`, compact or pretty, before and after it holds a verifier, a known name's or one an unknown name
/// took from the secret, holds neither the secret nor a key; it holds the principal it found.
#[test]
fn test_a_server_never_prints_its_secret_or_a_key() -> TestResult {
    let known = verifier()?;
    let unknown = ScramVerifier::unknown(&SECRET, NAME, ITERATIONS);
    let fresh = ScramServer::new(SERVER_NONCE, SECRET).with_iterations(ITERATIONS);
    let mut found = fresh.clone();
    let _found_first: Vec<u8> = found.first(CLIENT_FIRST, |_| Some((42, known.clone())))?;
    let mut refused = fresh.clone();
    let _refused_first: Vec<u8> = refused.first(CLIENT_FIRST, |_| None)?;
    for server in [&fresh, &found, &refused] {
        for shown in [format!("{server:?}"), format!("{server:#?}")] {
            hides(&shown, &SECRET)?;
            for verifier in [&known, &unknown] {
                hides(&shown, verifier.stored_key())?;
                hides(&shown, verifier.server_key())?;
            }
        }
    }
    let printed = format!("{found:?}");
    assert!(printed.contains("Some((Some(42)"), "{printed}");
    Ok(())
}
