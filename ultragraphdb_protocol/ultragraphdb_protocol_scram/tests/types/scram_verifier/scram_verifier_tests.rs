/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ultragraphdb_protocol_scram::{
    DEFAULT_ITERATIONS, MAX_ITERATIONS, ScramClient, ScramError, ScramRelay, ScramServer,
    ScramVerifier,
};

use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::rfc7677::{
    AUTH_MESSAGE, CLIENT_FINAL, CLIENT_FIRST, CLIENT_NONCE, CLIENT_PROOF, ITERATIONS, NAME,
    PASSWORD, SALT, SECRET, SERVER_FIRST, SERVER_KEY, SERVER_NONCE, SERVER_SIGNATURE, STORED_KEY,
    hex, verifier,
};

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

/// RFC 7677's ClientProof, as its client-final-message carries it.
fn rfc_proof() -> TestResult<[u8; 32]> {
    Ok(STANDARD
        .decode(CLIENT_PROOF)
        .context("RFC 7677's proof")?
        .as_slice()
        .try_into()?)
}

/// C1b.10: the right proof is signed with the ServerSignature RFC 7677 publishes and `ScramServer` sends, and the
/// client that sent the proof verifies it.
#[test]
fn test_a_proof_that_holds_is_signed_with_the_servers_signature() -> TestResult {
    let signature = verifier()?
        .sign(AUTH_MESSAGE, &rfc_proof()?)
        .context("RFC 7677's proof holds")?;
    assert_eq!(
        signature.to_vec(),
        STANDARD
            .decode(SERVER_SIGNATURE)
            .context("RFC 7677's signature")?
    );
    let mut server = ScramServer::new(SERVER_NONCE, SECRET);
    let known = verifier()?;
    assert_eq!(
        server.first(CLIENT_FIRST, |_| Some((1, known.clone())))?,
        SERVER_FIRST
    );
    let server_final = format!("v={}", STANDARD.encode(signature)).into_bytes();
    assert_eq!(server.last(CLIENT_FINAL)?, (server_final.clone(), 1));
    let mut client = ScramClient::new(NAME, PASSWORD, CLIENT_NONCE)?;
    assert_eq!(client.first_message()?, CLIENT_FIRST);
    assert_eq!(client.final_message(SERVER_FIRST)?, CLIENT_FINAL);
    client.verify(&server_final)?;
    Ok(())
}

/// C1b.9: the right proof with its first bit changed, or its last, is signed with nothing.
#[test]
fn test_a_proof_with_one_bit_changed_is_signed_with_nothing() -> TestResult {
    let known = verifier()?;
    for (byte, bit) in [(0, 0x80), (31, 0x01)] {
        let mut proof = rfc_proof()?;
        let changed = proof.get_mut(byte).context("a byte of the proof")?;
        *changed ^= bit;
        assert_eq!(known.sign(AUTH_MESSAGE, &proof), None);
    }
    // Another AuthMessage under the right proof is signed with nothing too.
    assert_eq!(
        known.sign(&format!("{AUTH_MESSAGE},x"), &rfc_proof()?),
        None
    );
    Ok(())
}

/// C1b.11: a name no principal holds, with any proof: its verifier signs nothing, the proof of the password a client
/// guesses as little as one of zeros.
#[test]
fn test_the_verifier_of_a_name_not_held_signs_nothing() -> TestResult {
    let unknown = ScramVerifier::unknown(&SECRET, "nobody", ITERATIONS);
    let mut client = ScramClient::new("nobody", PASSWORD, CLIENT_NONCE)?;
    let mut relay = ScramRelay::new(SERVER_NONCE);
    assert_eq!(relay.first(&client.first_message()?)?, "nobody");
    let server_first = relay.challenge(unknown.salt(), unknown.iterations())?;
    let (auth_message, proof) = relay.last(&client.final_message(&server_first)?)?;
    assert_eq!(unknown.sign(&auth_message, &proof), None);
    assert_eq!(unknown.sign(&auth_message, &[0; 32]), None);
    assert_eq!(unknown.sign(AUTH_MESSAGE, &rfc_proof()?), None);
    Ok(())
}

/// C1b.12: the verifier of a name not held, derived twice from one secret, is equal; from two secrets, or for two
/// names, it differs; it takes the count it is asked for.
#[test]
fn test_an_unknown_names_verifier_is_a_function_of_the_secret_and_the_name() -> TestResult {
    let derived = ScramVerifier::unknown(&SECRET, "nobody", ITERATIONS);
    assert_eq!(
        derived,
        ScramVerifier::unknown(&SECRET, "nobody", ITERATIONS)
    );
    assert_ne!(
        derived,
        ScramVerifier::unknown(&[8; 32], "nobody", ITERATIONS)
    );
    assert_ne!(
        derived,
        ScramVerifier::unknown(&SECRET, "someone", ITERATIONS)
    );
    assert_ne!(
        derived.salt(),
        ScramVerifier::unknown(&SECRET, "someone", ITERATIONS).salt()
    );
    assert_eq!(derived.iterations(), ITERATIONS);
    assert_eq!(
        ScramVerifier::unknown(&SECRET, "nobody", DEFAULT_ITERATIONS).iterations(),
        DEFAULT_ITERATIONS
    );
    Ok(())
}

/// C1b.12: an unknown name's salt and count are those `ScramServer` sends for the name under the same secret and
/// count: the relay's server-first-message of the derived verifier is the server's.
#[test]
fn test_an_unknown_names_salt_and_count_are_the_servers() -> TestResult {
    for (name, secret, iterations) in [
        ("nobody", SECRET, ITERATIONS),
        ("someone", [8; 32], DEFAULT_ITERATIONS),
    ] {
        let client_first = format!("n,,n={name},r={CLIENT_NONCE}");
        let unknown = ScramVerifier::unknown(&secret, name, iterations);
        let mut relay = ScramRelay::new(SERVER_NONCE);
        assert_eq!(relay.first(client_first.as_bytes())?, name);
        let built = relay.challenge(unknown.salt(), unknown.iterations())?;
        let sent = ScramServer::new(SERVER_NONCE, secret)
            .with_iterations(iterations)
            .first(client_first.as_bytes(), |_| None)?;
        assert_eq!(built, sent);
    }
    Ok(())
}
