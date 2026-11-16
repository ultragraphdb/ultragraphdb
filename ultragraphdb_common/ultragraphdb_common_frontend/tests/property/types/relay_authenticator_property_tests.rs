/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A relayed exchange: law L5.10 of `notes/laws.md`, against `ScramServer` holding the principal's verifier (design D8).

use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_common_frontend::RelayAuthenticator;
use ultragraphdb_protocol_scram::{MECHANISM, ScramClient, ScramServer, ScramVerifier};
use ultragraphdb_protocol_transport::{Authenticator, MechanismStep, ServerMechanism};

const LETTERS: &str = "abcdefghijklmnopqrstuvwxyz0123456789";
const PRINCIPAL: u64 = 41;
const SECRET: [u8; 32] = [7; 32];

/// What the relay answers the client's exchange of `password` as `name`, the group holding `held` for `named`: the
/// principal it authenticates as, or `None`.
fn relayed(name: &str, password: &str, named: &str, held: &ScramVerifier) -> Option<u64> {
    let mut relay = RelayAuthenticator::default();
    let mut mechanism = relay.start(MECHANISM)?;
    let mut client = ScramClient::new(name, password, "clientnonce").ok()?;
    let first = client.first_message().ok()?;
    let asked = relay.first(&first, "servernonce").ok()?;
    // The group's half: a held name's verifier, an unknown one's derived from the secret.
    let verifier = if asked == named {
        held.clone()
    } else {
        ScramVerifier::unknown(&SECRET, &asked, held.iterations())
    };
    relay.challenged(verifier.salt(), verifier.iterations());
    let Ok(MechanismStep::Continue(server_first)) = mechanism.step(&first) else {
        return None;
    };
    let last = client.final_message(&server_first).ok()?;
    let (auth_message, proof) = relay.last(&last).ok()?;
    match verifier.sign(&auth_message, &proof) {
        Some(signature) if asked == named => relay.verified(PRINCIPAL, &signature),
        Some(_) | None => relay.refused(),
    }
    match mechanism.step(&last) {
        Ok(MechanismStep::Done(server_final, credential)) => {
            client.verify(&server_final).ok()?;
            Some(credential.principal())
        }
        Ok(MechanismStep::Continue(_)) | Err(_) => None,
    }
}

/// What `ScramServer` holding `held` for `named` answers the same exchange.
fn served(name: &str, password: &str, named: &str, held: &ScramVerifier) -> Option<u64> {
    let mut server = ScramServer::new("servernonce", SECRET).with_iterations(held.iterations());
    let mut client = ScramClient::new(name, password, "clientnonce").ok()?;
    let first = client.first_message().ok()?;
    let server_first = server
        .first(&first, |asked| {
            (asked == named).then(|| (PRINCIPAL, held.clone()))
        })
        .ok()?;
    let last = client.final_message(&server_first).ok()?;
    let (server_final, principal) = server.last(&last).ok()?;
    client.verify(&server_final).ok()?;
    Some(principal)
}

/// L5.10: over drawn names and passwords, the relay authenticates exactly the exchanges `ScramServer` holding the
/// principal's verifier authenticates, as the same principal, and refuses a wrong proof and an unknown name alike.
#[hegel::test(test_cases = 12)]
fn test_the_relay_authenticates_as_the_server_does(tc: TestCase) {
    let named: String = tc.draw(gs::text().alphabet(LETTERS).min_size(1).max_size(12));
    let password: String = tc.draw(gs::text().alphabet(LETTERS).min_size(1).max_size(12));
    let held = ScramVerifier::derive(&password, b"a drawn salt", 4_096);
    assert!(held.is_ok());
    let Ok(held) = held else {
        return;
    };
    let name = if tc.draw(gs::booleans()) {
        named.clone()
    } else {
        tc.draw(gs::text().alphabet(LETTERS).min_size(1).max_size(12))
    };
    let tried = if tc.draw(gs::booleans()) {
        password.clone()
    } else {
        tc.draw(gs::text().alphabet(LETTERS).min_size(1).max_size(12))
    };
    let expected = served(&name, &tried, &named, &held);
    assert_eq!(relayed(&name, &tried, &named, &held), expected);
    assert_eq!(expected.is_some(), name == named && tried == password);
}
