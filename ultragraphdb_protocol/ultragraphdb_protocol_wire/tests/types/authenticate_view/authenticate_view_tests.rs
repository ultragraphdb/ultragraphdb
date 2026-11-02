/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{AuthenticateView, BodyView};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::authenticate::authenticate_tests::{CLIENT_FIRST, SERVER_FINAL, golden_exchange};
use crate::utils::golden_bodies::{
    frame_of, golden, join_sections, put, reframe, split_sections, with_command_header,
};

#[test]
fn test_the_golden_exchange_reads_its_values() -> TestResult {
    let first = golden("authenticate_client_first")?.bytes()?;
    let view = AuthenticateView::from_frame(&frame_of(&first)?)?;
    assert_eq!(view.step(), 0);
    assert!(!view.is_final());
    assert_eq!(view.mechanism(), "SCRAM-SHA-256");
    assert_eq!(view.message(), CLIENT_FIRST);
    let last = golden("authenticate_server_final")?.bytes()?;
    let last_view = AuthenticateView::from_frame(&frame_of(&last)?)?;
    assert_eq!(last_view.step(), 3);
    assert!(last_view.is_final());
    assert_eq!(last_view.principal(), 7);
    assert_eq!(last_view.expires(), 1_798_761_600_000_000);
    assert_eq!(last_view.mechanism(), "");
    assert_eq!(last_view.message(), SERVER_FINAL);
    let names = [
        "authenticate_client_first",
        "authenticate_server_first",
        "authenticate_client_final",
        "authenticate_server_final",
    ];
    for (message, name) in golden_exchange().into_iter().zip(names) {
        let bytes = golden(name)?.bytes()?;
        assert_eq!(
            AuthenticateView::from_frame(&frame_of(&bytes)?)?.to_body()?,
            message,
            "{name}"
        );
    }
    Ok(())
}

fn refused(name: &str, change: impl FnOnce(&mut [u8])) -> TestResult<ProtocolErrorEnum> {
    let bytes = with_command_header(&golden(name)?.bytes()?, change);
    Ok(AuthenticateView::from_frame(&frame_of(&bytes)?)
        .err()
        .context("a refusal")?
        .0)
}

#[test]
fn test_unused_bytes_are_zero() -> TestResult {
    for at in [5usize, 7, 24, 127] {
        assert_eq!(
            refused("authenticate_server_final", |h| h[at] = 1)?,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    Ok(())
}

#[test]
fn test_a_final_message_is_the_servers_and_alone_names_a_principal() -> TestResult {
    let field = |field: &'static str, value: u64| ProtocolErrorEnum::FieldValue {
        command: 18,
        field,
        value,
    };
    assert_eq!(
        refused("authenticate_server_final", |h| h[4] = 2)?,
        field("final", 2)
    );
    // A final message at an even step, a client's.
    assert_eq!(
        refused("authenticate_server_final", |h| put(h, 0, 2, 4))?,
        field("step", 2)
    );
    // A principal or an expiry outside a final message.
    assert_eq!(
        refused("authenticate_server_first", |h| put(h, 8, 7, 8))?,
        field("principal", 7)
    );
    assert_eq!(
        refused("authenticate_server_first", |h| put(h, 16, 9, 8))?,
        field("expires", 9)
    );
    // The fields in their order: step, final, principal, expires.
    assert!(matches!(
        refused("authenticate_server_first", |h| {
            put(h, 8, 7, 8);
            put(h, 16, 9, 8);
        })?,
        ProtocolErrorEnum::FieldValue {
            field: "principal",
            ..
        }
    ));
    // A final message with no principal is the anonymous one's, and never expiring is 0.
    let bytes = with_command_header(&golden("authenticate_server_final")?.bytes()?, |h| {
        put(h, 8, 0, 8);
        put(h, 16, 0, 8);
    });
    let view = AuthenticateView::from_frame(&frame_of(&bytes)?)?;
    assert!(view.is_final());
    assert_eq!((view.principal(), view.expires()), (0, 0));
    Ok(())
}

#[test]
fn test_the_mechanism_is_named_at_step_zero_alone() -> TestResult {
    // The client's first message without a mechanism, and a later message naming one.
    let first = golden("authenticate_client_first")?;
    let mut secs = split_sections(&first.data()?, 0, 2);
    secs[0].clear();
    let unnamed = reframe(&first.bytes()?, &join_sections(&[], &secs));
    assert_eq!(
        AuthenticateView::from_frame(&frame_of(&unnamed)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::FieldValue {
            command: 18,
            field: "mechanism",
            value: 0
        }
    );
    let later = golden("authenticate_client_final")?;
    let mut later_secs = split_sections(&later.data()?, 0, 2);
    later_secs[0] = b"SCRAM-SHA-256".to_vec();
    let named = reframe(&later.bytes()?, &join_sections(&[], &later_secs));
    assert_eq!(
        AuthenticateView::from_frame(&frame_of(&named)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::FieldValue {
            command: 18,
            field: "mechanism",
            value: 13
        }
    );
    // A mechanism name that is not UTF-8.
    let mut bad_name_secs = split_sections(&first.data()?, 0, 2);
    bad_name_secs[0][1] = 0xFF;
    let bad = reframe(&first.bytes()?, &join_sections(&[], &bad_name_secs));
    assert_eq!(
        AuthenticateView::from_frame(&frame_of(&bad)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::NotUtf8 {
            command: 18,
            section: 0,
            offset: 1
        }
    );
    // The mechanism's message is its own bytes, any of them.
    let mut any_bytes_secs = split_sections(&first.data()?, 0, 2);
    any_bytes_secs[1] = vec![0xFF, 0x00, 0xFE];
    let binary = reframe(&first.bytes()?, &join_sections(&[], &any_bytes_secs));
    assert_eq!(
        AuthenticateView::from_frame(&frame_of(&binary)?)?.message(),
        &[0xFF, 0x00, 0xFE]
    );
    Ok(())
}
