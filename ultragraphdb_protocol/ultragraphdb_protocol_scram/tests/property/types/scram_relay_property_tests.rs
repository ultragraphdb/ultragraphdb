/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The laws of the endpoint's half, `ScramRelay` (`notes/laws.md`, L1b.1, L1b.2, L1b.3, L1b.5, L1b.7 and L1b.8):
//! against `ScramServer` and `ScramClient` as they stand, and RFC 5802 §7's grammar of the server-final-message. Each
//! law runs in a function whose setup fails through `?`, so the test asserts that it ran to its end.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_protocol_scram::{
    ScramClient, ScramError, ScramRelay, ScramServer, ScramVerifier,
};

use crate::utils::draw::{Drawn, PRINCIPAL, SECRET, alter, exchange, nonce, salt, text};
use crate::utils::halves::Halves;

/// The halves of the drawn exchange, the group holding what `lookup` finds.
fn halves<L: Fn(&str) -> Option<(u64, ScramVerifier)>>(drawn: &Drawn, lookup: L) -> Halves<L> {
    Halves::new(
        &drawn.server_nonce,
        SECRET,
        drawn.verifier.iterations(),
        lookup,
    )
}

/// L1b.1: the drawn exchange run by its client against the two halves, to the client's check of the
/// server-final-message; the principal the halves name.
fn completes(tc: &TestCase) -> Result<u64, ScramError> {
    let drawn = exchange(tc, false)?;
    let mut client = drawn.client()?;
    let mut halves = halves(&drawn, drawn.lookup());
    let server_first = halves.first(&client.first_message()?)?;
    let (server_final, principal) = halves.last(&client.final_message(&server_first)?)?;
    client.verify(&server_final)?;
    Ok(principal)
}

/// L1b.1: a `ScramClient` holding the drawn password completes its exchange against the relay and the group's half,
/// and verifies the server-final-message.
#[hegel::test]
fn test_a_client_completes_its_exchange_through_the_two_halves(tc: TestCase) {
    assert_eq!(completes(&tc), Ok(PRINCIPAL));
}

/// One step of an exchange: a client-first or a client-final message, as the client sent it or changed in one place.
#[derive(Debug, Clone, Copy)]
enum Step {
    First { altered: bool },
    Last { altered: bool },
}

/// L1b.2: the drawn steps sent to the two halves and to `ScramServer`, each message answered alike.
fn steps_answered_alike(tc: &TestCase) -> Result<(), ScramError> {
    let drawn = exchange(tc, tc.draw(gs::booleans()))?;
    let held = tc.draw(gs::booleans());
    let lookup = |name: &str| drawn.lookup()(name).filter(|_| held);
    let mut server = drawn.server();
    let mut halves = halves(&drawn, lookup);
    let mut client = drawn.client()?;
    let client_first = client.first_message()?;
    let client_final = client.final_message(&drawn.server().first(&client_first, lookup)?)?;
    let steps = tc.draw(
        gs::vecs(gs::sampled_from(vec![
            Step::First { altered: false },
            Step::First { altered: true },
            Step::Last { altered: false },
            Step::Last { altered: true },
        ]))
        .min_size(1)
        .max_size(4),
    );
    for step in steps {
        match step {
            Step::First { altered } => {
                let message = if altered {
                    alter(tc, &client_first)
                } else {
                    client_first.clone()
                };
                assert_eq!(halves.first(&message), server.first(&message, lookup));
            }
            Step::Last { altered } => {
                let message = if altered {
                    alter(tc, &client_final)
                } else {
                    client_final.clone()
                };
                assert_eq!(halves.last(&message), server.last(&message));
            }
        }
    }
    Ok(())
}

/// L1b.2: a drawn sequence of the client's two messages, each right or changed in one place, sent to the two halves
/// and to `ScramServer` holding the same verifier, the name held or not and the password right or wrong: each message
/// is answered alike, so the two accept exactly the same exchanges.
#[hegel::test]
fn test_the_two_halves_accept_exactly_what_the_server_accepts(tc: TestCase) {
    assert_eq!(steps_answered_alike(&tc), Ok(()));
}

/// L1b.3: the relay's and `ScramServer`'s server-first-message for a drawn name, salt and count.
fn server_firsts(tc: &TestCase) -> Result<(Vec<u8>, Vec<u8>), ScramError> {
    let client_first = ScramClient::new(&text(tc, 12), "pw", &nonce(tc))?.first_message()?;
    let server_nonce = nonce(tc);
    let salt = salt(tc);
    let count = tc.draw(gs::integers::<u32>());
    let held = ScramVerifier::new(salt.clone(), count, [1; 32], [2; 32]);
    let mut relay = ScramRelay::new(&server_nonce);
    let _name: String = relay.first(&client_first)?;
    let relayed = relay.challenge(&salt, count)?;
    let served = ScramServer::new(&server_nonce, SECRET)
        .first(&client_first, |_| Some((PRINCIPAL, held.clone())))?;
    Ok((relayed, served))
}

/// L1b.3: the relay's server-first-message of a drawn salt and any count equals `ScramServer`'s for a verifier of that
/// salt and count, given the same nonces.
#[hegel::test]
fn test_the_server_first_message_is_the_servers(tc: TestCase) {
    let built = server_firsts(&tc);
    assert_eq!(
        built.as_ref().map(|(relayed, _)| relayed),
        built.as_ref().map(|(_, served)| served)
    );
    assert!(built.is_ok());
}

/// The relay of the drawn exchange once it has read the client's final message, with the AuthMessage and the proof.
fn relay_to_the_proof(drawn: &Drawn) -> Result<(ScramRelay, String, [u8; 32]), ScramError> {
    let mut client = drawn.client()?;
    let mut relay = ScramRelay::new(&drawn.server_nonce);
    let _name: String = relay.first(&client.first_message()?)?;
    let server_first = relay.challenge(drawn.verifier.salt(), drawn.verifier.iterations())?;
    let (auth_message, proof) = relay.last(&client.final_message(&server_first)?)?;
    Ok((relay, auth_message, proof))
}

/// L1b.3: the relay's server-final-messages of a drawn signature and of the group's, and `ScramServer`'s.
fn server_finals(tc: &TestCase) -> Result<(), ScramError> {
    let signature: [u8; 32] = tc.draw(gs::arrays(gs::integers::<u8>()));
    let drawn = exchange(tc, false)?;
    let (mut relay, _, _) = relay_to_the_proof(&drawn)?;
    assert_eq!(
        relay.finish(&signature)?,
        format!("v={}", STANDARD.encode(signature)).into_bytes()
    );
    let (mut proven, auth_message, proof) = relay_to_the_proof(&drawn)?;
    let signed = drawn
        .verifier
        .sign(&auth_message, &proof)
        .ok_or(ScramError::proof())?;
    let mut client = drawn.client()?;
    let mut server = drawn.server();
    let server_first = server.first(&client.first_message()?, drawn.lookup())?;
    let served = server.last(&client.final_message(&server_first)?)?;
    assert_eq!((proven.finish(&signed)?, PRINCIPAL), served);
    Ok(())
}

/// L1b.3: the relay's server-final-message of any signature is RFC 5802 §7's `v=` and the signature in base64, and of
/// the signature the group's half gives, `ScramServer`'s byte for byte.
#[hegel::test]
fn test_the_server_final_message_is_the_servers(tc: TestCase) {
    assert_eq!(server_finals(&tc), Ok(()));
}

/// Whether `message` holds `key` as a run of its 32 bytes, or as its base64 with or without padding.
fn holds(message: &[u8], key: &[u8; 32]) -> bool {
    let base64 = STANDARD.encode(key);
    let text = String::from_utf8_lossy(message);
    message.windows(32).any(|run| run == key)
        || text.contains(&base64)
        || text.contains(base64.trim_end_matches('='))
}

/// L1b.5: the messages the relay builds in a drawn exchange, and its `Debug`, checked for the two keys.
fn no_key_leaves(tc: &TestCase) -> Result<(), ScramError> {
    let wrong = tc.draw(gs::booleans());
    let drawn = exchange(tc, wrong)?;
    let mut client = drawn.client()?;
    let mut halves = halves(&drawn, drawn.lookup());
    let server_first = halves.first(&client.first_message()?)?;
    let answered = halves.last(&client.final_message(&server_first)?);
    assert_eq!(answered.is_err(), wrong);
    let server_final = answered.map(|(message, _)| message).unwrap_or_default();
    let debug = format!("{:?}", halves.relay());
    for key in [drawn.verifier.stored_key(), drawn.verifier.server_key()] {
        assert!(!holds(&server_first, key));
        assert!(!holds(&server_final, key));
        assert!(!holds(debug.as_bytes(), key));
        assert!(!debug.contains(&format!("{key:?}")));
    }
    Ok(())
}

/// L1b.5: no message the relay builds in a drawn exchange, right or with a wrong password, holds a run of 32 bytes
/// equal to the stored key or the server key, raw or in base64; nor does the relay's `Debug`.
#[hegel::test]
fn test_no_message_the_relay_builds_holds_a_key(tc: TestCase) {
    assert_eq!(no_key_leaves(&tc), Ok(()));
}

/// What `ScramServer` reads of `client_first`: the name it looks up, or its refusal.
fn server_reads(client_first: &[u8]) -> Result<Option<String>, ScramError> {
    let mut looked_up = None;
    let _server_first: Vec<u8> = ScramServer::new("s", SECRET).first(client_first, |name| {
        looked_up = Some(name.to_string());
        None
    })?;
    Ok(looked_up)
}

/// L1b.7: over a client-first-message valid, changed in one place, or any bytes, the relay reads what `ScramServer`
/// reads: the name it looks up, or the same refusal; and `ScramServer::name_in` reads the relay's name, and refuses
/// with the relay's refusal what it refuses (`notes/group-1b.md`, *Found wanting*, 1).
#[hegel::test]
fn test_the_relay_reads_the_client_first_message_as_the_server_does(tc: TestCase) {
    let built = ScramClient::new(&text(&tc, 12), "pw", &nonce(&tc))
        .and_then(|mut client| client.first_message());
    assert!(built.is_ok());
    let built = built.unwrap_or_default();
    let message = match tc.draw(gs::integers::<u8>().max_value(2)) {
        0 => built,
        1 => alter(&tc, &built),
        _ => tc.draw(gs::binary().max_size(48)),
    };
    let read = ScramRelay::new("s").first(&message);
    assert_eq!(read.clone().map(Some), server_reads(&message));
    match (ScramServer::name_in(&message), &read) {
        (Ok(named), Ok(name)) => assert_eq!(&named, name),
        (Err(refusal), _) => assert_eq!(Err(refusal), read),
        (Ok(_), Err(refusal)) => assert_eq!(refusal, &ScramError::malformed("nonce")),
    }
}

/// The client-final-message `client_final` changed in one drawn place: its nonce, its channel binding, its proof's
/// encoding or length, or an attribute added; or a byte anywhere.
fn final_altered(tc: &TestCase, client_final: &str) -> Vec<u8> {
    let (without_proof, proof) = client_final
        .rsplit_once(",p=")
        .unwrap_or((client_final, ""));
    let (binding, rest) = without_proof.split_once(',').unwrap_or((without_proof, ""));
    match tc.draw(gs::integers::<u8>().max_value(5)) {
        0 => format!("{binding},r={},p={proof}", nonce(tc)).into_bytes(),
        1 => format!("c={},{rest},p={proof}", text(tc, 8)).into_bytes(),
        2 => format!("{without_proof},p={proof}{}", text(tc, 3)).into_bytes(),
        3 => {
            let mut resized = STANDARD.decode(proof).unwrap_or_default();
            resized.resize(tc.draw(gs::integers::<usize>().max_value(40)), 0);
            format!("{without_proof},p={}", STANDARD.encode(resized)).into_bytes()
        }
        4 => format!("{without_proof},{}=x,p={proof}", text(tc, 2)).into_bytes(),
        _ => alter(tc, client_final.as_bytes()),
    }
}

/// L1b.8: a drawn client-final-message read by the relay and answered by `ScramServer`.
fn finals_refused_alike(tc: &TestCase) -> Result<(), ScramError> {
    let drawn = exchange(tc, tc.draw(gs::booleans()))?;
    let mut client = drawn.client()?;
    let client_first = client.first_message()?;
    let mut relay = ScramRelay::new(&drawn.server_nonce);
    let mut server = drawn.server();
    let server_first = server.first(&client_first, drawn.lookup())?;
    assert_eq!(relay.first(&client_first)?, drawn.name);
    assert_eq!(
        relay.challenge(drawn.verifier.salt(), drawn.verifier.iterations())?,
        server_first
    );
    let client_final = client.final_message(&server_first)?;
    let message = if tc.draw(gs::booleans()) {
        final_altered(tc, &String::from_utf8_lossy(&client_final))
    } else {
        client_final
    };
    let expected = relay.last(&message).and_then(|(auth_message, proof)| {
        drawn
            .verifier
            .sign(&auth_message, &proof)
            .map(|signature| {
                let server_final = format!("v={}", STANDARD.encode(signature));
                (server_final.into_bytes(), PRINCIPAL)
            })
            .ok_or(ScramError::proof())
    });
    assert_eq!(server.last(&message), expected);
    Ok(())
}

/// L1b.8: over a client-final-message valid or changed in one drawn place, the relay refuses exactly what
/// `ScramServer::last` refuses, with the same refusal; what the relay reads, `ScramServer` answers as the group's
/// signature of it decides.
#[hegel::test]
fn test_the_relay_refuses_the_client_final_message_as_the_server_does(tc: TestCase) {
    assert_eq!(finals_refused_alike(&tc), Ok(()));
}
