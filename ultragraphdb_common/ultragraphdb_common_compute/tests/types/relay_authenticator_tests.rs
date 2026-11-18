/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A client's exchange relayed through the frontend, its proof checked by the half that holds the verifier
//! (`PROTOCOL.md` §11.1, §13.8, §13.9; design D8).

use ultragraphdb_common_compute::RelayAuthenticator;
use ultragraphdb_protocol_scram::{MECHANISM, ScramClient, ScramVerifier};
use ultragraphdb_protocol_transport::{Authenticator, MechanismStep, ServerMechanism};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

const ANN: u64 = 41;
const NONCE: &str = "theservernonce";

fn verifier() -> TestResult<ScramVerifier> {
    Ok(ScramVerifier::derive("pw", b"a salt of ann", 4_096)?)
}

/// The exchange of `password` against `ann`'s verifier through the relay: the principal and the server-final-message
/// when the relay authenticates, `None` when its step fails.
fn exchange(password: &str) -> TestResult<Option<(u64, Vec<u8>)>> {
    let held = verifier()?;
    let mut relay = RelayAuthenticator::default();
    let mut mechanism = relay.start(MECHANISM).context("the mechanism")?;
    let mut client = ScramClient::new("ann", password, "theclientnonce")?;
    let first = client.first_message()?;
    let name = relay.first(&first, NONCE)?;
    assert_eq!(name, "ann");
    assert_eq!(relay.name().as_deref(), Some("ann"));
    relay.challenged(held.salt(), held.iterations());
    let Ok(MechanismStep::Continue(server_first)) = mechanism.step(&first) else {
        return Err(TestError::new("the server-first-message"));
    };
    let last = client.final_message(&server_first)?;
    let (auth_message, proof) = relay.last(&last)?;
    match held.sign(&auth_message, &proof) {
        Some(signature) => relay.verified(ANN, &signature),
        None => relay.refused(),
    }
    match mechanism.step(&last) {
        Ok(MechanismStep::Done(server_final, credential)) => {
            client.verify(&server_final)?;
            Ok(Some((credential.principal(), server_final)))
        }
        Ok(MechanismStep::Continue(_)) => Err(TestError::new("a third step")),
        Err(_) => Ok(None),
    }
}

/// The right password authenticates as the principal the group named, the client verifying the server's signature.
#[test]
fn test_the_right_password_authenticates_through_the_relay() -> TestResult {
    let (principal, _) = exchange("pw")?.context("authenticated")?;
    assert_eq!(principal, ANN);
    Ok(())
}

/// A wrong password fails the exchange's last step.
#[test]
fn test_a_wrong_password_fails_the_exchange() -> TestResult {
    assert_eq!(exchange("wrong")?, None);
    Ok(())
}

/// A step none was prepared for fails, and another mechanism is not offered.
#[test]
fn test_an_unprepared_step_fails() -> TestResult {
    let mut relay = RelayAuthenticator::default();
    assert!(relay.start("PLAIN").is_none());
    let mut mechanism = relay.start(MECHANISM).context("the mechanism")?;
    assert!(mechanism.step(b"n,,n=ann,r=abc").is_err());
    Ok(())
}

/// A malformed client-first-message is refused by the relay, and its step fails.
#[test]
fn test_a_malformed_first_message_is_refused() -> TestResult {
    let mut relay = RelayAuthenticator::default();
    let mut mechanism = relay.start(MECHANISM).context("the mechanism")?;
    assert!(relay.first(b"no scram here", NONCE).is_err());
    relay.refused();
    assert!(mechanism.step(b"no scram here").is_err());
    Ok(())
}

/// Its `Debug` holds no message text: neither the name nor the nonce.
#[test]
fn test_its_debug_holds_no_message_text() -> TestResult {
    let relay = RelayAuthenticator::default();
    let mut client = ScramClient::new("annabel", "pw", "theclientnonce")?;
    let _name = relay.first(&client.first_message()?, NONCE)?;
    let printed = format!("{relay:?}");
    assert!(!printed.contains("annabel"), "{printed}");
    assert!(!printed.contains(NONCE), "{printed}");
    Ok(())
}
