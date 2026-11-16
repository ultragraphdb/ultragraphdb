/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as a client-first-message (RFC 5802, RFC 7677) read by `ScramRelay`, the endpoint's half of a server's
//! exchange. Nothing panics; the message is refused with a typed error or gives the name a Challenge carries; the relay
//! reads what `ScramServer` reads, the name it looks up or the same refusal, and `ScramServer::name_in` reads the same
//! name; an accepted message is answered with the server-first-message `ScramServer` builds for the same salt and count.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_protocol_scram::{
    MIN_ITERATIONS, ScramError, ScramRelay, ScramServer, ScramVerifier,
};

/// RFC 7677's server nonce and salt.
const SERVER_NONCE: &str = "%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0";
const SALT: [u8; 16] = [
    0x5b, 0x6d, 0x99, 0x68, 0x9d, 0x12, 0x35, 0x8e, 0xec, 0xa0, 0x4b, 0x14, 0x12, 0x36, 0xfa, 0x81,
];
const SECRET: [u8; 32] = [7; 32];

fuzz_target!(|client_first: &[u8]| {
    let mut relay = ScramRelay::new(SERVER_NONCE);
    let read = relay.first(client_first);
    let mut looked_up = None;
    let held = ScramVerifier::new(SALT.to_vec(), MIN_ITERATIONS, [1; 32], [2; 32]);
    let served = ScramServer::new(SERVER_NONCE, SECRET).first(client_first, |name| {
        looked_up = Some(name.to_string());
        Some((1, held))
    });
    match (&read, &served) {
        (Ok(name), Ok(_)) => assert_eq!(looked_up.as_ref(), Some(name)),
        (Err(refusal), Err(served)) => assert_eq!(refusal, served),
        _ => panic!("the relay read {read:?} where the server answered {served:?}"),
    }
    match (ScramServer::name_in(client_first), &read) {
        (Ok(named), Ok(name)) => assert_eq!(&named, name),
        (Ok(_), Err(refusal)) => assert_eq!(*refusal, ScramError::malformed("nonce")),
        (Err(refusal), _) => assert_eq!(read.as_ref().err(), Some(&refusal)),
    }
    if let Ok(server_first) = served {
        assert_eq!(
            relay.challenge(&SALT, MIN_ITERATIONS).ok(),
            Some(server_first)
        );
    }
});
