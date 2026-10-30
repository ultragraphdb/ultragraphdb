/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! RFC 7677's SCRAM-SHA-256 exchange (section 3), and its keys computed with Python's hashlib and hmac, an oracle
//! independent of the crate, which reproduces the RFC's proof and signature from them.

pub const NAME: &str = "user";
pub const PASSWORD: &str = "pencil";
pub const CLIENT_NONCE: &str = "rOprNGfwEbeRWgbNEkqO";
pub const SERVER_NONCE: &str = "%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0";
/// `W22ZaJ0SNY7soEsUEjb6gQ==`.
pub const SALT: [u8; 16] = [
    0x5b, 0x6d, 0x99, 0x68, 0x9d, 0x12, 0x35, 0x8e, 0xec, 0xa0, 0x4b, 0x14, 0x12, 0x36, 0xfa, 0x81,
];
pub const ITERATIONS: u32 = 4_096;
pub const STORED_KEY: &str = "586e5df283e6dceb5c3e791d8b8528ec191e664045ce971792e2e6b5bb13e2a6";
pub const SERVER_KEY: &str = "c1f3cbc1c13a9d35a14c0990eed97629ea225863e566a4314ab99f3f00e5d9d5";

pub const CLIENT_FIRST: &[u8] = b"n,,n=user,r=rOprNGfwEbeRWgbNEkqO";
pub const SERVER_FIRST: &[u8] =
    b"r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096";
pub const CLIENT_FINAL: &[u8] = b"c=biws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,\
p=dHzbZapWIk4jUhN+Ute9ytag9zjfMHgsqmmiz7AndVQ=";
pub const SERVER_FINAL: &[u8] = b"v=6rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4=";

pub const SECRET: [u8; 32] = [7; 32];

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn verifier() -> ultragraphdb_protocol_scram::ScramVerifier {
    ultragraphdb_protocol_scram::ScramVerifier::derive(PASSWORD, &SALT, ITERATIONS).unwrap()
}
