/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_scram::{Credentials, ScramAuthenticator, ScramVerifier};
use ultragraphdb_protocol_transport::{Authenticator, MechanismStep, ServerMechanism};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::hidden::hides;
use crate::utils::rfc7677::{CLIENT_FIRST, CLIENT_NONCE, ITERATIONS, SERVER_NONCE};

const SECRET: [u8; 32] = *b"the server's secret of 32 bytes.";

/// A store holding no principal, so every name is answered from the secret.
#[derive(Debug, Clone)]
struct Empty;

impl Credentials for Empty {
    fn lookup(&self, _name: &str) -> Option<(u64, ScramVerifier)> {
        None
    }
}

fn nonce() -> String {
    SERVER_NONCE.to_string()
}

/// An authenticator's `Debug`, and that of an exchange it started past the client's first message, compact or pretty,
/// hold neither the secret nor the keys of the verifier an unknown name took from it, nor a nonce or the text of a
/// message, the client-first-message-bare or the server-first-message, which the exchange prints as their lengths.
#[test]
fn test_an_authenticator_and_its_exchange_never_print_the_secret() -> TestResult {
    let nonces: fn() -> String = nonce;
    let mut authenticator =
        ScramAuthenticator::new(Empty, nonces, SECRET).with_iterations(ITERATIONS);
    let mut exchange = authenticator
        .start("SCRAM-SHA-256")
        .context("an exchange of SCRAM-SHA-256")?;
    let server_first = match exchange.step(CLIENT_FIRST)? {
        MechanismStep::Continue(message) => Some(String::from_utf8(message)?),
        MechanismStep::Done(..) => None,
    }
    .context("the server-first-message")?;
    let bare = std::str::from_utf8(CLIENT_FIRST)?
        .strip_prefix("n,,")
        .context("the client-first-message after its GS2 header")?;
    let unknown = ScramVerifier::unknown(&SECRET, "user", ITERATIONS);
    let printed = [
        format!("{authenticator:?}"),
        format!("{authenticator:#?}"),
        format!("{exchange:?}"),
        format!("{exchange:#?}"),
    ];
    for shown in &printed {
        hides(shown, &SECRET)?;
        hides(shown, unknown.stored_key())?;
        hides(shown, unknown.server_key())?;
        for text in [SERVER_NONCE, CLIENT_NONCE, bare, server_first.as_str()] {
            hides(shown, text.as_bytes())?;
        }
    }
    let iterations = format!("iterations: {ITERATIONS}");
    assert!(printed[0].contains(&iterations), "{}", printed[0]);
    assert!(printed[2].contains(&iterations), "{}", printed[2]);
    for expected in [
        format!("nonce: <{} bytes>", SERVER_NONCE.len()),
        format!("server_first: <{} bytes>", server_first.len()),
    ] {
        printed[2]
            .contains(&expected)
            .then_some(())
            .context(&format!("{} holds {expected}", printed[2]))?;
    }
    Ok(())
}
