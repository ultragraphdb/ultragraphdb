/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as a client's client-first and client-final messages (RFC 5802, RFC 7677), the first's length a
//! little-endian u16 before them, read by a server of RFC 7677's nonce that holds RFC 7677's user. Nothing panics; a
//! message out of step is refused; an accepted client-first is answered with the client's nonce extended by the
//! server's; only the user's proof authenticates, as its principal, and is answered with a server signature.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_protocol_scram::{MIN_ITERATIONS, ScramServer, ScramVerifier};

/// RFC 7677's server nonce, so the RFC's client-final message, the seed's, carries the nonce this server expects.
const SERVER_NONCE: &str = "%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0";
const SECRET: [u8; 32] = [7; 32];
const PRINCIPAL: u64 = 7;
/// RFC 7677's salt and keys for the password `pencil` at 4,096 iterations.
const SALT: [u8; 16] = [
    0x5b, 0x6d, 0x99, 0x68, 0x9d, 0x12, 0x35, 0x8e, 0xec, 0xa0, 0x4b, 0x14, 0x12, 0x36, 0xfa, 0x81,
];
const STORED_KEY: [u8; 32] = [
    0x58, 0x6e, 0x5d, 0xf2, 0x83, 0xe6, 0xdc, 0xeb, 0x5c, 0x3e, 0x79, 0x1d, 0x8b, 0x85, 0x28, 0xec,
    0x19, 0x1e, 0x66, 0x40, 0x45, 0xce, 0x97, 0x17, 0x92, 0xe2, 0xe6, 0xb5, 0xbb, 0x13, 0xe2, 0xa6,
];
const SERVER_KEY: [u8; 32] = [
    0xc1, 0xf3, 0xcb, 0xc1, 0xc1, 0x3a, 0x9d, 0x35, 0xa1, 0x4c, 0x09, 0x90, 0xee, 0xd9, 0x76, 0x29,
    0xea, 0x22, 0x58, 0x63, 0xe5, 0x66, 0xa4, 0x31, 0x4a, 0xb9, 0x9f, 0x3f, 0x00, 0xe5, 0xd9, 0xd5,
];

/// The principal and verifier of RFC 7677's user, the only name the server holds.
fn lookup(name: &str) -> Option<(u64, ScramVerifier)> {
    (name == "user").then(|| {
        (
            PRINCIPAL,
            ScramVerifier::new(SALT.to_vec(), 4_096, STORED_KEY, SERVER_KEY),
        )
    })
}

fuzz_target!(|data: &[u8]| {
    let Some((len, rest)) = data.split_first_chunk::<2>() else {
        return;
    };
    let split = usize::from(u16::from_le_bytes(*len)).min(rest.len());
    let (client_first, client_final) = rest.split_at(split);
    assert!(
        ScramServer::new(SERVER_NONCE, SECRET)
            .last(client_final)
            .is_err(),
        "a client-final message before the first is out of step"
    );
    let mut server = ScramServer::new(SERVER_NONCE, SECRET).with_iterations(MIN_ITERATIONS);
    let Ok(server_first) = server.first(client_first, lookup) else {
        return;
    };
    let text = std::str::from_utf8(&server_first).expect("a server-first message is text");
    let nonce = text
        .split(',')
        .next()
        .and_then(|r| r.strip_prefix("r="))
        .expect("a server-first message opens with the nonce");
    assert!(nonce.ends_with(SERVER_NONCE) && nonce.len() > SERVER_NONCE.len());
    assert!(
        server.first(client_first, lookup).is_err(),
        "a second client-first message is out of step"
    );
    if let Ok((server_final, principal)) = server.last(client_final) {
        assert_eq!(principal, PRINCIPAL, "only the user's proof authenticates");
        assert!(server_final.starts_with(b"v="));
    }
});
