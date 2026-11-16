/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ultragraphdb_protocol_scram::ScramClient;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::hidden::hides;
use crate::utils::rfc7677::{
    CLIENT_NONCE, NAME, PASSWORD, SERVER_FINAL, SERVER_FIRST, SERVER_SIGNATURE,
};

/// `client`'s `Debug`, compact and pretty.
fn shown(client: &ScramClient) -> [String; 2] {
    [format!("{client:?}"), format!("{client:#?}")]
}

/// A client's `Debug` holds its name and its nonce's length at every step and never the password or the nonce, nor,
/// once it built its first message, the client-first-message-bare, which it prints as its length, nor, once it built
/// its final message, the server signature it expects.
#[test]
fn test_a_client_never_prints_its_password_or_the_signature_it_expects() -> TestResult {
    let signature = STANDARD.decode(SERVER_SIGNATURE)?;
    let mut client = ScramClient::new(NAME, PASSWORD, CLIENT_NONCE)?;
    let mut states = vec![shown(&client)];
    let first = String::from_utf8(client.first_message()?)?;
    let bare = first
        .strip_prefix("n,,")
        .context("the client-first-message after its GS2 header")?;
    let at_first = shown(&client);
    states.push(at_first.clone());
    let _final: Vec<u8> = client.final_message(SERVER_FIRST)?;
    states.push(shown(&client));
    client.verify(SERVER_FINAL)?;
    states.push(shown(&client));
    let nonce_length = format!("nonce: <{} bytes>", CLIENT_NONCE.len());
    for [printed, pretty] in &states {
        for text in [printed, pretty] {
            hides(text, PASSWORD.as_bytes())?;
            hides(text, &signature)?;
            hides(text, CLIENT_NONCE.as_bytes())?;
            hides(text, bare.as_bytes())?;
        }
        assert!(printed.contains(&format!("name: {NAME:?}")), "{printed}");
        printed
            .contains(&nonce_length)
            .then_some(())
            .context(&format!("{printed} holds {nonce_length}"))?;
    }
    let bare_length = format!("client_first_bare: <{} bytes>", bare.len());
    at_first[0]
        .contains(&bare_length)
        .then_some(())
        .context(&format!("{} holds {bare_length}", at_first[0]))?;
    Ok(())
}
