/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{Authenticate, BodyBuilder};

use crate::utils::golden_bodies::golden;

/// RFC 7677's SCRAM-SHA-256 exchange, section 3.
pub const CLIENT_FIRST: &[u8] = b"n,,n=user,r=rOprNGfwEbeRWgbNEkqO";
pub const SERVER_FIRST: &[u8] =
    b"r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096";
pub const CLIENT_FINAL: &[u8] = b"c=biws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,\
p=dHzbZapWIk4jUhN+Ute9ytag9zjfMHgsqmmiz7AndVQ=";
pub const SERVER_FINAL: &[u8] = b"v=6rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4=";

/// The golden exchange's four messages, in order.
pub fn golden_exchange() -> [Authenticate; 4] {
    [
        Authenticate::new(0, "SCRAM-SHA-256", CLIENT_FIRST.to_vec()),
        Authenticate::new(1, "", SERVER_FIRST.to_vec()),
        Authenticate::new(2, "", CLIENT_FINAL.to_vec()),
        Authenticate::new(3, "", SERVER_FINAL.to_vec()).with_final(7, 1_798_761_600_000_000),
    ]
}

#[test]
fn test_the_exchange_builds_the_golden_frames() {
    let names = [
        "authenticate_client_first",
        "authenticate_server_first",
        "authenticate_client_final",
        "authenticate_server_final",
    ];
    for (message, name) in golden_exchange().iter().zip(names) {
        assert_eq!(
            message.frame(golden(name).header()).unwrap(),
            golden(name).bytes(),
            "{name}"
        );
    }
}

#[test]
fn test_a_message_keeps_its_fields() {
    let first = Authenticate::new(0, "SCRAM-SHA-256", b"m".to_vec());
    assert_eq!(first.step(), 0);
    assert!(!first.is_final());
    assert_eq!((first.principal(), first.expires()), (0, 0));
    assert_eq!(first.mechanism(), "SCRAM-SHA-256");
    assert_eq!(first.message(), b"m");
    let last = Authenticate::new(3, "", Vec::new()).with_final(0x0102, 0x0304);
    assert!(last.is_final());
    assert_eq!((last.principal(), last.expires()), (0x0102, 0x0304));
    // The command header by the table: step at 0, final at 4, principal at 8, expires at 16.
    let header = last.command_header();
    assert_eq!(&header[..4], &3u32.to_le_bytes());
    assert_eq!(header[4], 1);
    assert_eq!(&header[5..8], &[0, 0, 0]);
    assert_eq!(&header[8..16], &0x0102u64.to_le_bytes());
    assert_eq!(&header[16..24], &0x0304u64.to_le_bytes());
    assert!(header[24..].iter().all(|b| *b == 0));
}
