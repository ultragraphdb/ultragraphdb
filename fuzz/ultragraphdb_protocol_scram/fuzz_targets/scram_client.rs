/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as a server's server-first and server-final messages (RFC 5802, RFC 7677), the first's length a
//! little-endian u16 before them, read by RFC 7677's client. Nothing panics; a message out of step is refused; an
//! accepted server-first message is answered with a client-final message carrying the server's nonce and a proof; the
//! server-final message is checked or refused. A count outside 4,096 to 10,000,000 is refused before a key is derived,
//! so no input holds the client longer than the ceiling's work.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_protocol_scram::ScramClient;

const CLIENT_NONCE: &str = "rOprNGfwEbeRWgbNEkqO";
const CLIENT_FIRST: &[u8] = b"n,,n=user,r=rOprNGfwEbeRWgbNEkqO";

fuzz_target!(|data: &[u8]| {
    let Some((len, rest)) = data.split_first_chunk::<2>() else {
        return;
    };
    let split = usize::from(u16::from_le_bytes(*len)).min(rest.len());
    let (server_first, server_final) = rest.split_at(split);
    let mut client = ScramClient::new("user", "pencil", CLIENT_NONCE).expect("RFC 7677's client");
    assert!(
        client.final_message(server_first).is_err(),
        "a server-first message before the client's is out of step"
    );
    assert_eq!(
        client.first_message().expect("the first message"),
        CLIENT_FIRST
    );
    let Ok(client_final) = client.final_message(server_first) else {
        return;
    };
    let nonce = std::str::from_utf8(server_first)
        .ok()
        .and_then(|text| text.split(',').next())
        .and_then(|r| r.strip_prefix("r="))
        .expect("an accepted server-first message opens with its nonce");
    assert!(nonce.starts_with(CLIENT_NONCE) && nonce.len() > CLIENT_NONCE.len());
    let text = std::str::from_utf8(&client_final).expect("a client-final message is text");
    assert!(text.starts_with(&format!("c=biws,r={nonce},p=")));
    let _ = client.verify(server_final);
});
