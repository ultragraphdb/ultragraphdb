/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_scram::{MAX_ITERATIONS, ScramError, ScramVerifier};

use ultragraphdb_tool_bdd::TestResult;

use crate::utils::rfc7677::{ITERATIONS, PASSWORD, SALT, SERVER_KEY, STORED_KEY, hex, verifier};

#[test]
fn test_a_verifier_derives_rfc_7677s_keys() -> TestResult {
    let derived = verifier()?;
    assert_eq!(derived.salt(), &SALT);
    assert_eq!(derived.iterations(), ITERATIONS);
    assert_eq!(hex(derived.stored_key()), STORED_KEY);
    assert_eq!(hex(derived.server_key()), SERVER_KEY);
    let stored = ScramVerifier::new(
        SALT.to_vec(),
        ITERATIONS,
        *derived.stored_key(),
        *derived.server_key(),
    );
    assert_eq!(
        stored, derived,
        "a verifier read back from its store is the derived one"
    );
    Ok(())
}

#[test]
fn test_saslprep_maps_before_the_key_and_refuses_what_it_prohibits() -> TestResult {
    // A soft hyphen maps to nothing (RFC 4013 section 2.2): the same key as without it.
    let mapped = ScramVerifier::derive("pen\u{00AD}cil", &SALT, ITERATIONS)?;
    assert_eq!(mapped, verifier()?);
    // A non-ASCII space maps to an ASCII space.
    let spaced = ScramVerifier::derive("a\u{00A0}b", &SALT, ITERATIONS)?;
    assert_eq!(spaced, ScramVerifier::derive("a b", &SALT, ITERATIONS)?);
    // A control character is prohibited.
    assert_eq!(
        ScramVerifier::derive("pen\u{0007}cil", &SALT, ITERATIONS),
        Err(ScramError::prepare())
    );
    assert_ne!(
        ScramVerifier::derive(PASSWORD, &SALT, ITERATIONS + 1)?,
        verifier()?,
        "another count, another verifier"
    );
    Ok(())
}

#[test]
fn test_fewer_iterations_than_the_least_are_refused() -> TestResult {
    assert_eq!(
        ScramVerifier::derive(PASSWORD, &SALT, 4_095),
        Err(ScramError::iterations(4_095))
    );
    assert!(ScramVerifier::derive(PASSWORD, &SALT, 4_096).is_ok());
    Ok(())
}

#[test]
fn test_more_iterations_than_a_client_runs_are_refused() -> TestResult {
    // A verifier no client would authenticate against is never made.
    assert_eq!(
        ScramVerifier::derive(PASSWORD, &SALT, MAX_ITERATIONS + 1),
        Err(ScramError::iterations(MAX_ITERATIONS + 1))
    );
    Ok(())
}
