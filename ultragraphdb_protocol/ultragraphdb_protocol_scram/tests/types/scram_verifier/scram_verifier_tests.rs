/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_scram::{ScramError, ScramVerifier};

use crate::utils::rfc7677::{ITERATIONS, PASSWORD, SALT, SERVER_KEY, STORED_KEY, hex, verifier};

#[test]
fn test_a_verifier_derives_rfc_7677s_keys() {
    let derived = verifier();
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
}

#[test]
fn test_saslprep_maps_before_the_key_and_refuses_what_it_prohibits() {
    // A soft hyphen maps to nothing (RFC 4013 section 2.2): the same key as without it.
    let mapped = ScramVerifier::derive("pen\u{00AD}cil", &SALT, ITERATIONS).unwrap();
    assert_eq!(mapped, verifier());
    // A non-ASCII space maps to an ASCII space.
    let spaced = ScramVerifier::derive("a\u{00A0}b", &SALT, ITERATIONS).unwrap();
    assert_eq!(
        spaced,
        ScramVerifier::derive("a b", &SALT, ITERATIONS).unwrap()
    );
    // A control character is prohibited.
    assert_eq!(
        ScramVerifier::derive("pen\u{0007}cil", &SALT, ITERATIONS),
        Err(ScramError::Prepare())
    );
    assert_ne!(
        ScramVerifier::derive(PASSWORD, &SALT, ITERATIONS + 1).unwrap(),
        verifier(),
        "another count, another verifier"
    );
}

#[test]
fn test_fewer_iterations_than_the_least_are_refused() {
    assert_eq!(
        ScramVerifier::derive(PASSWORD, &SALT, 4_095),
        Err(ScramError::Iterations(4_095))
    );
    assert!(ScramVerifier::derive(PASSWORD, &SALT, 4_096).is_ok());
}
