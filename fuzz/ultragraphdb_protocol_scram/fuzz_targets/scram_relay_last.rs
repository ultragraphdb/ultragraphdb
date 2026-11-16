/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as a client-final-message (RFC 5802, RFC 7677) read by `ScramRelay` after RFC 7677's client-first-message
//! and server-first-message. Nothing panics; the message is refused with a typed error or gives the AuthMessage and a
//! ClientProof of 32 bytes, the AuthMessage opening with the exchange's first two messages; the relay refuses exactly
//! what `ScramServer` holding RFC 7677's user refuses, with the same refusal, and what it reads `ScramServer` answers
//! as the group's signature of it decides; only a signed proof is answered with a server-final-message.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_protocol_scram::{ScramError, ScramRelay, ScramServer, ScramVerifier};

/// RFC 7677's exchange: its nonces, salt, keys and first two messages.
const SERVER_NONCE: &str = "%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0";
const CLIENT_FIRST: &[u8] = b"n,,n=user,r=rOprNGfwEbeRWgbNEkqO";
const SERVER_FIRST: &str =
    "r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096";
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
const SECRET: [u8; 32] = [7; 32];
const PRINCIPAL: u64 = 7;

fuzz_target!(|client_final: &[u8]| {
    let verifier = ScramVerifier::new(SALT.to_vec(), 4_096, STORED_KEY, SERVER_KEY);
    let mut relay = ScramRelay::new(SERVER_NONCE);
    assert_eq!(relay.first(CLIENT_FIRST).as_deref(), Ok("user"));
    assert_eq!(
        relay.challenge(&SALT, 4_096).ok(),
        Some(SERVER_FIRST.as_bytes().to_vec())
    );
    let mut server = ScramServer::new(SERVER_NONCE, SECRET);
    let first = server.first(CLIENT_FIRST, |_| Some((PRINCIPAL, verifier.clone())));
    assert_eq!(first.ok(), Some(SERVER_FIRST.as_bytes().to_vec()));
    let read = relay.last(client_final);
    let signed = read.clone().and_then(|(auth_message, proof)| {
        assert!(
            auth_message.starts_with(&format!("n=user,r=rOprNGfwEbeRWgbNEkqO,{SERVER_FIRST},c="))
        );
        verifier
            .sign(&auth_message, &proof)
            .ok_or(ScramError::proof())
    });
    let served = server.last(client_final);
    match (signed, &served) {
        (Ok(signature), Ok((server_final, principal))) => {
            assert_eq!(*principal, PRINCIPAL);
            assert_eq!(relay.finish(&signature).as_ref(), Ok(server_final));
        }
        (Err(refusal), Err(served)) => assert_eq!(&refusal, served),
        (signed, served) => {
            panic!("the halves gave {signed:?} where the server answered {served:?}")
        }
    }
});
