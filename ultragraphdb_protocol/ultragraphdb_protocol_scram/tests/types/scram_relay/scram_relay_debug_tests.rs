/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ultragraphdb_protocol_scram::ScramRelay;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::hidden::hides;
use crate::utils::rfc7677::{
    CLIENT_FINAL, CLIENT_FIRST, CLIENT_NONCE, ITERATIONS, SALT, SERVER_NONCE, verifier,
};

/// `Ok` when `printed` holds `expected`.
fn holds(printed: &str, expected: &str) -> TestResult {
    printed
        .contains(expected)
        .then_some(())
        .context(&format!("{printed} holds {expected}"))
}

/// A relay's `Debug`, compact or pretty, at every step of RFC 7677's exchange, holds no message it read or built, no
/// part of one it keeps, the channel binding, the salt or a nonce; it holds the length of each it keeps.
#[test]
fn test_a_relay_prints_the_length_of_its_messages_never_their_text() -> TestResult {
    let mut relay = ScramRelay::new(SERVER_NONCE);
    let initial = relay.clone();
    let _name = relay.first(CLIENT_FIRST)?;
    let read = relay.clone();
    let server_first = String::from_utf8(relay.challenge(&SALT, ITERATIONS)?)?;
    let challenged = relay.clone();
    let (auth_message, proof) = relay.last(CLIENT_FINAL)?;
    let answered = relay.clone();
    let signature = verifier()?
        .sign(&auth_message, &proof)
        .context("RFC 7677's proof holds")?;
    let _server_final = relay.finish(&signature)?;
    let client_first = std::str::from_utf8(CLIENT_FIRST)?;
    let bare = client_first
        .strip_prefix("n,,")
        .context("the client-first-message after its GS2 header")?;
    let binding = STANDARD.encode("n,,");
    let nonce = server_first
        .strip_prefix("r=")
        .and_then(|rest| rest.split(',').next())
        .context("the server-first-message's nonce")?;
    let client_final = std::str::from_utf8(CLIENT_FINAL)?;
    let texts = [
        SERVER_NONCE,
        CLIENT_NONCE,
        nonce,
        client_first,
        bare,
        binding.as_str(),
        server_first.as_str(),
        auth_message.as_str(),
        client_final,
    ];
    let relays = [&initial, &read, &challenged, &answered, &relay];
    for each in relays {
        for shown in [format!("{each:?}"), format!("{each:#?}")] {
            for text in texts {
                hides(&shown, text.as_bytes())?;
            }
            hides(&shown, &SALT)?;
        }
        holds(
            &format!("{each:?}"),
            &format!("nonce: <{} bytes>", SERVER_NONCE.len()),
        )?;
    }
    for each in [&read, &challenged, &answered] {
        let printed = format!("{each:?}");
        holds(&printed, &format!("binding: <{} bytes>", binding.len()))?;
        holds(
            &printed,
            &format!("client_first_bare: <{} bytes>", bare.len()),
        )?;
        holds(&printed, &format!("nonce: <{} bytes>", nonce.len()))?;
    }
    for each in [&challenged, &answered] {
        holds(
            &format!("{each:?}"),
            &format!("server_first: <{} bytes>", server_first.len()),
        )?;
    }
    Ok(())
}
