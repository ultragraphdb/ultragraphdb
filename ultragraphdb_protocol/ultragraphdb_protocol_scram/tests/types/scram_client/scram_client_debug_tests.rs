/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ultragraphdb_protocol_scram::ScramClient;
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::hidden::hides;
use crate::utils::rfc7677::{
    CLIENT_NONCE, NAME, PASSWORD, SERVER_FINAL, SERVER_FIRST, SERVER_SIGNATURE,
};

/// `client`'s `Debug`, compact and pretty.
fn shown(client: &ScramClient) -> [String; 2] {
    [format!("{client:?}"), format!("{client:#?}")]
}

/// A client's `Debug` holds its name and nonce at every step and never the password, nor, once it built its final
/// message, the server signature it expects.
#[test]
fn test_a_client_never_prints_its_password_or_the_signature_it_expects() -> TestResult {
    let signature = STANDARD.decode(SERVER_SIGNATURE)?;
    let mut client = ScramClient::new(NAME, PASSWORD, CLIENT_NONCE)?;
    let mut states = vec![shown(&client)];
    let _first: Vec<u8> = client.first_message()?;
    states.push(shown(&client));
    let _final: Vec<u8> = client.final_message(SERVER_FIRST)?;
    states.push(shown(&client));
    client.verify(SERVER_FINAL)?;
    states.push(shown(&client));
    for [printed, pretty] in &states {
        for text in [printed, pretty] {
            hides(text, PASSWORD.as_bytes())?;
            hides(text, &signature)?;
        }
        assert!(printed.contains(&format!("name: {NAME:?}")), "{printed}");
        assert!(
            printed.contains(&format!("nonce: {CLIENT_NONCE:?}")),
            "{printed}"
        );
    }
    Ok(())
}
