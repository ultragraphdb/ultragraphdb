/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The scenarios of group 1b that the two halves of the exchange decide without a connection: system-group, "A
//! client's proof is checked in the system group", and client-endpoint, "Connections authenticate against the metadata
//! group's principals". A client's exchange runs through [`ScramRelay`], as the frontend runs it, and the system
//! group answers Challenge and Verify (`PROTOCOL.md` §13.8, §13.9) from the principal's verifier, or an unknown name's,
//! and [`ScramVerifier::sign`].

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use cucumber::{World, then, when};
use ultragraphdb_protocol_scram::{
    MIN_ITERATIONS, ScramClient, ScramError, ScramRelay, ScramVerifier,
};
use ultragraphdb_protocol_wire::{BodyBuilder, Challenge, Verify};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

/// `ann`, the one principal the group holds, her password, salt and principal number.
const ANN: &str = "ann";
const ANN_PASSWORD: &str = "correct horse battery staple";
const ANN_SALT: &[u8] = b"ann's salt, 16 b";
const ANN_PRINCIPAL: u64 = 9;
/// The secret the group derives an unknown name's verifier from.
const SECRET: [u8; 32] = [7; 32];
const CLIENT_NONCE: &str = "client-nonce";

/// One exchange as the frontend relays it: the system group's two replies, the messages the relay built, and how it
/// ended.
#[derive(Debug)]
struct Relayed {
    challenge: Challenge,
    verify: Verify,
    built: Vec<Vec<u8>>,
    outcome: Result<u64, ScramError>,
}

/// The state the scenarios carry from one step to the next.
#[derive(Debug, Default, World)]
pub struct Steps {
    exchanges: Vec<Relayed>,
}

/// `ann`'s verifier, as the group holds it.
fn ann() -> TestResult<ScramVerifier> {
    Ok(ScramVerifier::derive(
        ANN_PASSWORD,
        ANN_SALT,
        MIN_ITERATIONS,
    )?)
}

/// The principal and verifier the group answers `name` with: `ann`'s, or an unknown name's from its secret.
fn held(name: &str) -> TestResult<(u64, ScramVerifier)> {
    if name == ANN {
        Ok((ANN_PRINCIPAL, ann()?))
    } else {
        Ok((0, ScramVerifier::unknown(&SECRET, name, MIN_ITERATIONS)))
    }
}

/// The exchange of a client of `name` and `password` through a frontend whose relay draws `server_nonce`: the relay
/// asks the group with a Challenge and a Verify, and passes on what they answer.
fn authenticate(name: &str, password: &str, server_nonce: &str) -> TestResult<Relayed> {
    let mut client = ScramClient::new(name, password, CLIENT_NONCE)?;
    let mut relay = ScramRelay::new(server_nonce);
    let asked = relay.first(&client.first_message()?)?;
    let (principal, verifier) = held(&asked)?;
    let challenge = Challenge::request(asked.clone())
        .with_salt(verifier.iterations(), verifier.salt().to_vec());
    let server_first = relay.challenge(challenge.salt(), challenge.iterations())?;
    let (auth_message, proof) = relay.last(&client.final_message(&server_first)?)?;
    let request = Verify::request(asked, auth_message.clone(), proof);
    let verify = match verifier.sign(&auth_message, &proof) {
        Some(signature) if principal != 0 => request.with_verified(principal, signature),
        _ => request,
    };
    let mut built = vec![server_first];
    let outcome = match verify.signature() {
        Some(signature) => {
            let server_final = relay.finish(signature)?;
            client.verify(&server_final)?;
            built.push(server_final);
            Ok(verify.principal())
        }
        None => Err(ScramError::proof()),
    };
    Ok(Relayed {
        challenge,
        verify,
        built,
        outcome,
    })
}

/// The bytes of a reply's body: its command header and its sections.
fn bytes<B: BodyBuilder>(body: &B) -> Vec<u8> {
    let mut bytes = body.command_header().to_vec();
    bytes.extend(body.body());
    bytes
}

/// Whether `bytes` hold `key` as a run of its 32 bytes, or as its base64 with or without padding.
fn holds(bytes: &[u8], key: &[u8; 32]) -> bool {
    let base64 = STANDARD.encode(key);
    let text = String::from_utf8_lossy(bytes);
    bytes.windows(32).any(|run| run == key)
        || text.contains(&base64)
        || text.contains(base64.trim_end_matches('='))
}

/// Neither `ann`'s stored key nor her server key in the replies of `relayed`, nor in a message its relay built.
fn holds_no_key(relayed: &Relayed) -> TestResult {
    let ann = ann()?;
    let replies = [bytes(&relayed.challenge), bytes(&relayed.verify)];
    for key in [ann.stored_key(), ann.server_key()] {
        for sent in replies.iter().chain(&relayed.built) {
            assert!(!holds(sent, key));
        }
    }
    Ok(())
}

// System group: a client's proof is checked in the system group.

#[when(
    "a Verify carries `ann`'s name, the exchange's AuthMessage and the proof `ann`'s password gives"
)]
fn verify_ann(w: &mut Steps) -> TestResult {
    w.exchanges.push(authenticate(ANN, ANN_PASSWORD, "server")?);
    Ok(())
}

#[then("the reply holds `verified` 1, `ann`'s principal and a ServerSignature of 32 bytes")]
fn verified_with_signature(w: &mut Steps) -> TestResult {
    let relayed = w.exchanges.first().context("an exchange")?;
    assert!(relayed.verify.verified());
    assert_eq!(relayed.verify.principal(), ANN_PRINCIPAL);
    let signature = relayed.verify.signature().context("a ServerSignature")?;
    assert_eq!(signature.len(), 32);
    // The client checked the signature in the server-final-message the relay built from it.
    assert_eq!(relayed.outcome, Ok(ANN_PRINCIPAL));
    let server_final = relayed.built.get(1).context("a server-final-message")?;
    assert_eq!(
        server_final,
        format!("v={}", STANDARD.encode(signature)).as_bytes()
    );
    Ok(())
}

#[when(
    "one Verify carries a wrong proof for `ann` and another any proof for the unknown name `nobody`"
)]
fn verify_wrong_and_unknown(w: &mut Steps) -> TestResult {
    w.exchanges
        .push(authenticate(ANN, "a wrong password", "server")?);
    w.exchanges
        .push(authenticate("nobody", ANN_PASSWORD, "server")?);
    Ok(())
}

#[then("both replies hold `verified` 0, `principal` 0 and no ServerSignature")]
fn both_unverified(w: &mut Steps) -> TestResult {
    assert_eq!(w.exchanges.len(), 2);
    for relayed in &w.exchanges {
        assert!(!relayed.verify.verified());
        assert_eq!(relayed.verify.principal(), 0);
        assert_eq!(relayed.verify.signature(), None);
    }
    Ok(())
}

#[when("a Challenge and a Verify for `ann` are answered")]
fn answer_ann(w: &mut Steps) -> TestResult {
    w.exchanges.push(authenticate(ANN, ANN_PASSWORD, "server")?);
    Ok(())
}

#[then(
    "neither reply holds `ann`'s stored key or server key, and no other command the endpoint serves returns them"
)]
fn no_reply_holds_a_key(w: &mut Steps) -> TestResult {
    let relayed = w.exchanges.first().context("an exchange")?;
    assert!(relayed.verify.verified());
    assert_eq!(relayed.built.len(), 2);
    holds_no_key(relayed)
}

// Client endpoint: connections authenticate against the metadata group's principals.

#[when("a client authenticates as `ann` with a wrong password, and as an unknown name")]
fn authenticate_wrong_and_unknown(w: &mut Steps) -> TestResult {
    w.exchanges
        .push(authenticate(ANN, "a wrong password", "server")?);
    w.exchanges
        .push(authenticate("nobody", ANN_PASSWORD, "server")?);
    Ok(())
}

#[then("both are refused with the same authentication error")]
fn refused_alike(w: &mut Steps) -> TestResult {
    let [wrong, unknown] = w.exchanges.as_slice() else {
        return Err(TestError::new("two exchanges"));
    };
    assert_eq!(wrong.outcome, Err(ScramError::proof()));
    assert_eq!(unknown.outcome, wrong.outcome);
    // The two Verify replies differ in no field the frontend reads.
    assert_eq!(
        (
            unknown.verify.verified(),
            unknown.verify.principal(),
            unknown.verify.signature()
        ),
        (
            wrong.verify.verified(),
            wrong.verify.principal(),
            wrong.verify.signature()
        )
    );
    Ok(())
}

#[when(
    "a client authenticates as `ann` and the frames the frontend receives from the system group are read"
)]
fn authenticate_ann(w: &mut Steps) -> TestResult {
    w.exchanges.push(authenticate(ANN, ANN_PASSWORD, "server")?);
    Ok(())
}

#[then(
    "they hold `ann`'s salt, iteration count, principal and the exchange's ServerSignature, and neither her stored key nor her server key"
)]
fn frames_hold_the_verifiers_public_part(w: &mut Steps) -> TestResult {
    let relayed = w.exchanges.first().context("an exchange")?;
    assert_eq!(relayed.challenge.salt(), ANN_SALT);
    assert_eq!(relayed.challenge.iterations(), MIN_ITERATIONS);
    assert_eq!(relayed.verify.principal(), ANN_PRINCIPAL);
    let signature = relayed.verify.signature().context("a ServerSignature")?;
    let server_final = relayed.built.get(1).context("a server-final-message")?;
    assert_eq!(
        server_final,
        format!("v={}", STANDARD.encode(signature)).as_bytes()
    );
    holds_no_key(relayed)
}

#[when("a client asks to authenticate as an unknown name, twice, across a restart of the frontend")]
fn unknown_twice(w: &mut Steps) -> TestResult {
    // Each start of the frontend is a relay of its own, drawing its own server nonce; the group keeps its secret.
    w.exchanges
        .push(authenticate("nobody", ANN_PASSWORD, "first-start")?);
    w.exchanges
        .push(authenticate("nobody", ANN_PASSWORD, "second-start")?);
    w.exchanges
        .push(authenticate(ANN, "a wrong password", "second-start")?);
    Ok(())
}

#[then(
    "both challenges carry the same salt and iteration count, and the exchange fails with the same authentication error as a wrong password"
)]
fn unknown_answered_as_known(w: &mut Steps) -> TestResult {
    let [first, second, wrong] = w.exchanges.as_slice() else {
        return Err(TestError::new("three exchanges"));
    };
    assert_eq!(first.challenge.salt(), second.challenge.salt());
    assert_eq!(first.challenge.iterations(), second.challenge.iterations());
    assert_eq!(first.challenge.salt().len(), wrong.challenge.salt().len());
    assert_eq!(first.challenge.iterations(), wrong.challenge.iterations());
    assert_eq!(first.outcome, Err(ScramError::proof()));
    assert_eq!(second.outcome, wrong.outcome);
    Ok(())
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
