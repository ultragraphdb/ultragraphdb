/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};
use ultragraphdb_protocol_wire::{Command, FrameView, MAX_FRAME_SIZE};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden::{CLUSTER, CREDIT, GOLDEN, HELLO, READ, put, reseal};

fn validate(bytes: &[u8]) -> Result<FrameView<'_>, ProtocolErrorEnum> {
    FrameView::validate(bytes, CLUSTER, MAX_FRAME_SIZE).map_err(|e| e.0)
}

#[test]
fn test_golden_frames_validate() -> TestResult {
    for golden in GOLDEN {
        let bytes = golden.bytes()?;
        let frame = validate(&bytes)
            .map_err(ProtocolError)
            .context("a golden frame")?;
        assert_eq!(frame.command().number(), golden.command, "{}", golden.name);
        assert_eq!(frame.body(), &golden.data()?[..], "{}", golden.name);
        assert_eq!(frame.chunked_body().data(), frame.body(), "{}", golden.name);
        assert_eq!(frame.header().size(), golden.size, "{}", golden.name);
        assert_eq!(
            frame.header().checksum(),
            golden.checksum,
            "{}",
            golden.name
        );
    }
    let bytes = READ.bytes()?;
    assert_eq!(
        validate(&bytes).map_err(ProtocolError)?.command(),
        Command::Read
    );
    Ok(())
}

#[test]
fn test_every_truncation_is_refused_cleanly() -> TestResult {
    for golden in GOLDEN {
        let bytes = golden.bytes()?;
        for len in 0..bytes.len() {
            let want = if len < 256 {
                ProtocolErrorEnum::ShortHeader { len }
            } else {
                ProtocolErrorEnum::LengthMismatch {
                    size: golden.size,
                    len,
                }
            };
            assert_eq!(
                validate(&bytes[..len]).err(),
                Some(want),
                "{} at {len}",
                golden.name
            );
        }
    }
    Ok(())
}

#[test]
fn test_bytes_past_the_frame_are_refused() -> TestResult {
    for golden in GOLDEN {
        let mut bytes = golden.bytes()?;
        bytes.push(0);
        assert_eq!(
            validate(&bytes).err(),
            Some(ProtocolErrorEnum::LengthMismatch {
                size: golden.size,
                len: golden.size as usize + 1
            }),
            "{}",
            golden.name
        );
    }
    Ok(())
}

#[test]
fn test_every_bit_flip_is_refused() -> TestResult {
    let golden = HELLO.bytes()?;
    for bit in 0..golden.len() * 8 {
        let mut bytes = golden.clone();
        bytes[bit / 8] ^= 1 << (bit % 8);
        let refused = validate(&bytes).err();
        if bit < 256 * 8 {
            assert!(
                matches!(refused, Some(ProtocolErrorEnum::HeaderChecksum { .. })),
                "bit {bit}: {refused:?}"
            );
        } else {
            assert_eq!(
                refused,
                Some(ProtocolErrorEnum::Chunk {
                    index: 0,
                    start: 0,
                    end: 100
                }),
                "bit {bit}"
            );
        }
    }
    Ok(())
}

#[test]
fn test_a_body_length_with_no_split_is_refused_naming_it() -> TestResult {
    let mut bytes = HELLO.bytes()?[..256].to_vec();
    bytes.extend_from_slice(&[0xA5; 3_841]);
    put(&mut bytes, 40, 256 + 3_841, 4);
    reseal(&mut bytes);
    assert_eq!(
        FrameView::validate(&bytes, CLUSTER, MAX_FRAME_SIZE),
        Err(ProtocolError(ProtocolErrorEnum::BodyLength { len: 3_841 }))
    );
    Ok(())
}

#[test]
fn test_the_length_is_checked_before_the_body() -> TestResult {
    // A truncated frame whose body is also corrupt: the length is the rule reported.
    let mut bytes = READ.bytes()?;
    bytes[10_000] ^= 1;
    bytes.truncate(20_000);
    assert_eq!(
        validate(&bytes).err(),
        Some(ProtocolErrorEnum::LengthMismatch {
            size: 20_736,
            len: 20_000
        })
    );
    Ok(())
}

#[test]
fn test_the_header_is_checked_before_the_length() -> TestResult {
    let mut bytes = READ.bytes()?;
    bytes.truncate(1_000);
    bytes[50] ^= 1;
    assert!(matches!(
        validate(&bytes),
        Err(ProtocolErrorEnum::HeaderChecksum { .. })
    ));
    let mut truncated = READ.bytes()?;
    truncated.truncate(1_000);
    assert_eq!(
        FrameView::validate(&truncated, 1, MAX_FRAME_SIZE).err(),
        Some(ProtocolError(ProtocolErrorEnum::WrongCluster {
            expected: 1,
            found: CLUSTER
        }))
    );
    Ok(())
}

#[test]
fn test_a_refusal_leaves_the_next_frame_unaffected() -> TestResult {
    let golden = CREDIT.bytes()?;
    let mut other = golden.clone();
    put(&mut other, 32, 42, 8);
    reseal(&mut other);
    assert_eq!(
        validate(&other).err(),
        Some(ProtocolErrorEnum::WrongCluster {
            expected: CLUSTER,
            found: 42
        })
    );
    assert_eq!(
        validate(&golden).map_err(ProtocolError)?.command(),
        Command::Credit
    );
    Ok(())
}

fn validate_client(bytes: &[u8]) -> Result<FrameView<'_>, ProtocolErrorEnum> {
    FrameView::validate_client(bytes, CLUSTER, MAX_FRAME_SIZE).map_err(|e| e.0)
}

#[test]
fn test_a_clients_frame_is_validated_with_the_clients_header_rules() -> TestResult {
    // The golden Hello is a client's frame: view and replica 0.
    let golden = HELLO.bytes()?;
    let frame = validate_client(&golden)
        .map_err(ProtocolError)
        .context("a client's golden frame")?;
    assert_eq!(frame.command(), Command::Hello);
    assert_eq!(frame.body(), &HELLO.data()?[..]);
    // PROTOCOL.md section 2.5 step 6: in a client's frame `view` and `replica` are 0; a server's frame takes any.
    let mut viewed = golden.clone();
    put(&mut viewed, 44, 7, 4);
    reseal(&mut viewed);
    assert_eq!(
        validate_client(&viewed).err(),
        Some(ProtocolErrorEnum::FieldValue {
            command: 1,
            field: "view",
            value: 7
        })
    );
    assert_eq!(validate(&viewed).map(|f| f.header().view()), Ok(7));
    let mut replica = golden.clone();
    replica[64] = 3;
    reseal(&mut replica);
    assert_eq!(
        validate_client(&replica).err(),
        Some(ProtocolErrorEnum::FieldValue {
            command: 1,
            field: "replica",
            value: 3
        })
    );
    // Past the header the client's frame takes the same steps: the length, then the chunks.
    assert_eq!(
        validate_client(&golden[..300]).err(),
        Some(ProtocolErrorEnum::LengthMismatch {
            size: 356,
            len: 300
        })
    );
    let mut flipped = golden.clone();
    flipped[300] ^= 1;
    // HELLO's body is one chunk of its 100 data bytes.
    assert_eq!(
        validate_client(&flipped).err(),
        Some(ProtocolErrorEnum::Chunk {
            index: 0,
            start: 0,
            end: 100
        })
    );
    assert_eq!(
        validate_client(&golden[..10]).err(),
        Some(ProtocolErrorEnum::ShortHeader { len: 10 })
    );
    Ok(())
}
