/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The laws of the group's half on `ScramVerifier` (`notes/laws.md`, L1b.4 and L1b.6): against `ScramServer` as it
//! stands. Each law runs in a function whose setup fails through `?`, so the test asserts that it ran to its end.

use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_protocol_scram::{
    ScramClient, ScramError, ScramRelay, ScramServer, ScramVerifier,
};

use crate::utils::draw::{SECRET, exchange, nonce, text};

/// The AuthMessage and the proof the relay reads from a client of the drawn name and password, the server-first-message
/// built from `verifier`.
fn proof_of(
    client: &mut ScramClient,
    server_nonce: &str,
    verifier: &ScramVerifier,
) -> Result<(String, [u8; 32]), ScramError> {
    let mut relay = ScramRelay::new(server_nonce);
    let _name: String = relay.first(&client.first_message()?)?;
    let server_first = relay.challenge(verifier.salt(), verifier.iterations())?;
    relay.last(&client.final_message(&server_first)?)
}

/// L1b.4: the signatures the group's half gives for the right proof, the right proof with one drawn bit changed, and
/// a name no principal holds.
fn refused_alike(tc: &TestCase) -> Result<(), ScramError> {
    let drawn = exchange(tc, false)?;
    let (auth_message, proof) =
        proof_of(&mut drawn.client()?, &drawn.server_nonce, &drawn.verifier)?;
    assert!(drawn.verifier.sign(&auth_message, &proof).is_some());
    let bit = tc.draw(gs::integers::<usize>().max_value(255));
    let mut changed = proof;
    if let Some(byte) = changed.get_mut(bit / 8) {
        *byte ^= 1 << (bit % 8);
    }
    let wrong = drawn.verifier.sign(&auth_message, &changed);
    let unknown = ScramVerifier::unknown(&SECRET, &drawn.name, drawn.verifier.iterations());
    let (unknown_message, unknown_proof) =
        proof_of(&mut drawn.client()?, &drawn.server_nonce, &unknown)?;
    let not_held = unknown.sign(&unknown_message, &unknown_proof);
    assert_eq!(wrong, None);
    assert_eq!(not_held, None);
    assert_eq!(wrong, not_held);
    Ok(())
}

/// L1b.4: a proof with one drawn bit changed and a name no principal holds are refused alike: the group's half gives
/// no ServerSignature for either, and nothing it returns tells them apart; the right proof is signed.
#[hegel::test]
fn test_a_changed_proof_and_a_name_not_held_are_refused_alike(tc: TestCase) {
    assert_eq!(refused_alike(&tc), Ok(()));
}

/// L1b.6: an unknown name's verifier derived twice, and the server-first-messages of it and of `ScramServer`.
fn unknown_is_the_servers(tc: &TestCase) -> Result<(), ScramError> {
    let secret: [u8; 32] = tc.draw(gs::arrays(gs::integers::<u8>()));
    let other: [u8; 32] = tc.draw(gs::arrays(gs::integers::<u8>()));
    let name = text(tc, 12);
    let iterations = tc.draw(gs::integers::<u32>());
    let derived = ScramVerifier::unknown(&secret, &name, iterations);
    assert_eq!(derived, ScramVerifier::unknown(&secret, &name, iterations));
    assert_eq!(derived.iterations(), iterations);
    if other != secret {
        assert_ne!(derived, ScramVerifier::unknown(&other, &name, iterations));
    }
    let server_nonce = nonce(tc);
    let client_first = ScramClient::new(&name, "pw", &nonce(tc))?.first_message()?;
    let mut relay = ScramRelay::new(&server_nonce);
    assert_eq!(relay.first(&client_first)?, name);
    let relayed = relay.challenge(derived.salt(), derived.iterations())?;
    let served = ScramServer::new(&server_nonce, secret)
        .with_iterations(iterations)
        .first(&client_first, |_| None)?;
    assert_eq!(relayed, served);
    Ok(())
}

/// L1b.6: the verifier of an unknown name is a function of the secret and the name: derived twice it is equal, under
/// another secret it differs, and its salt and count are those `ScramServer` sends for the name under the same secret.
#[hegel::test]
fn test_an_unknown_names_verifier_is_the_servers(tc: TestCase) {
    assert_eq!(unknown_is_the_servers(&tc), Ok(()));
}
