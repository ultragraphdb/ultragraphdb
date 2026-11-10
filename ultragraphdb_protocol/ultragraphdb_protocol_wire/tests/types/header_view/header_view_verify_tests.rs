/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};
use ultragraphdb_protocol_wire::{Command, Header, HeaderView, MAX_FRAME_SIZE};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden::{CLUSTER, GOLDEN, HELLO, READ, put, reseal};

/// The header of `bytes` verified as a receiver does; the outer result fails the test when `bytes` holds no header.
fn verify(bytes: &[u8]) -> TestResult<Result<Header, ProtocolErrorEnum>> {
    Ok(HeaderView::new(bytes)
        .context("a header")?
        .verify(CLUSTER, MAX_FRAME_SIZE)
        .map_err(|e| e.0))
}

/// The header of `bytes` verified as a client does; the outer result fails the test when `bytes` holds no header.
fn verify_client(bytes: &[u8]) -> TestResult<Result<Header, ProtocolErrorEnum>> {
    Ok(HeaderView::new(bytes)
        .context("a header")?
        .verify_client(CLUSTER, MAX_FRAME_SIZE)
        .map_err(|e| e.0))
}

/// HELLO's header with `edit` applied and its checksum recomputed.
fn resealed(edit: impl FnOnce(&mut Vec<u8>)) -> TestResult<Vec<u8>> {
    let mut bytes = HELLO.bytes()?[..256].to_vec();
    edit(&mut bytes);
    reseal(&mut bytes);
    Ok(bytes)
}

#[test]
fn test_golden_headers_verify_to_their_fields() -> TestResult {
    for golden in GOLDEN {
        let header = verify(&golden.bytes()?)?
            .map_err(ProtocolError)
            .context("a golden header verifies")?;
        let want = Header::new(CLUSTER, Command::from_number(golden.command)?)
            .with_sequence(golden.sequence)
            .with_view(golden.view)
            .with_release(golden.release)
            .with_protocol(golden.protocol)
            .with_stream(golden.stream)
            .with_replica(golden.replica)
            .with_flags(golden.flags)
            .with_command_header(golden.command_header());
        assert_eq!(header, want, "{}", golden.name);
    }
    Ok(())
}

#[test]
fn test_a_changed_checksum_names_both_values() -> TestResult {
    let mut bytes = HELLO.bytes()?;
    bytes[0] ^= 0x01;
    assert_eq!(
        verify(&bytes)?,
        Err(ProtocolErrorEnum::HeaderChecksum {
            stored: 0x51C7_F42A_F2F6_FA26,
            computed: 0x51C7_F42A_F2F6_FA27
        })
    );
    // Byte 100 flipped: the reference's XXH3-64 of the changed bytes 8..256 (Python's xxhash, libxxhash 0.8.2).
    let mut flipped = HELLO.bytes()?;
    flipped[100] ^= 0x01;
    assert_eq!(
        verify(&flipped)?,
        Err(ProtocolErrorEnum::HeaderChecksum {
            stored: 0x51C7_F42A_F2F6_FA27,
            computed: 0x5B5C_A6CD_D419_97E8
        })
    );
    Ok(())
}

#[test]
fn test_every_bit_flip_in_the_header_fails_its_checksum() -> TestResult {
    let golden = READ.bytes()?;
    for bit in 0..256 * 8 {
        let mut bytes = golden.clone();
        bytes[bit / 8] ^= 1 << (bit % 8);
        assert!(
            matches!(
                verify(&bytes)?,
                Err(ProtocolErrorEnum::HeaderChecksum { .. })
            ),
            "bit {bit}"
        );
    }
    Ok(())
}

#[test]
fn test_an_unknown_command_is_refused_by_number() -> TestResult {
    for number in [0u16, 26, 255, 256, 511, 532, 767, u16::MAX] {
        let bytes = resealed(|b| put(b, 54, number.into(), 2))?;
        assert_eq!(
            verify(&bytes)?,
            Err(ProtocolErrorEnum::UnknownCommand { command: number })
        );
    }
    // Section 3's client numbers from Authenticate to ConditionalWrite, Purge and Migrate, the highest, and the first
    // and last role commands are known.
    for (number, command) in [
        (18u16, Command::Authenticate),
        (19, Command::Drop),
        (20, Command::DirectWrite),
        (21, Command::ConditionalWrite),
        (24, Command::Purge),
        (25, Command::Migrate),
        (512, Command::Join),
        (530, Command::Report),
    ] {
        let bytes = resealed(|b| put(b, 54, number.into(), 2))?;
        assert_eq!(
            verify(&bytes)?.map(|h| h.command()),
            Ok(command),
            "command {number}"
        );
    }
    Ok(())
}

#[test]
fn test_each_reserved_byte_must_be_zero() -> TestResult {
    for offset in 66..128 {
        let bytes = resealed(|b| b[offset] = 0x01)?;
        assert_eq!(
            verify(&bytes)?,
            Err(ProtocolErrorEnum::ReservedNotZero { offset }),
            "offset {offset}"
        );
    }
    Ok(())
}

#[test]
fn test_the_first_nonzero_reserved_byte_is_named() -> TestResult {
    let bytes = resealed(|b| {
        b[100] = 1;
        b[70] = 1;
    })?;
    assert_eq!(
        verify(&bytes)?,
        Err(ProtocolErrorEnum::ReservedNotZero { offset: 70 })
    );
    Ok(())
}

#[test]
fn test_the_sequence_takes_any_value() -> TestResult {
    // Bytes 16 to 32 are the sequence: the header's rules take every value, its receiver checks the order.
    for sequence in [0u128, 1, 2, u128::from(u64::MAX) + 1, u128::MAX] {
        let bytes = resealed(|b| b[16..32].copy_from_slice(&sequence.to_le_bytes()))?;
        assert_eq!(
            verify(&bytes)?.map(|h| h.sequence()),
            Ok(sequence),
            "sequence {sequence}"
        );
    }
    Ok(())
}

#[test]
fn test_the_neighbours_of_the_reserved_bytes_are_not_reserved() -> TestResult {
    // Byte 65 is `flags`, whose bit 0 is free, and 128 starts the command header: either may be set.
    let bytes = resealed(|b| {
        b[65] = 0x01;
        b[128] = 0xFF;
        b[255] = 0xFF;
    })?;
    let header = verify(&bytes)?
        .map_err(ProtocolError)
        .context("LAST and the command header are free")?;
    assert_eq!(header.flags(), 0x01);
    // Byte 15 ends `checksum_body` and 32 starts `cluster`: they are not reserved bytes either.
    let neighbours = resealed(|b| b[15] ^= 0x80)?;
    assert!(verify(&neighbours)?.is_ok());
    Ok(())
}

#[test]
fn test_another_cluster_is_refused_naming_both() -> TestResult {
    let bytes = resealed(|b| put(b, 32, 0x0123_4567_89AB_CDEE, 8))?;
    assert_eq!(
        verify(&bytes)?,
        Err(ProtocolErrorEnum::WrongCluster {
            expected: CLUSTER,
            found: 0x0123_4567_89AB_CDEE
        })
    );
    let golden = HELLO.bytes()?;
    let view = HeaderView::new(&golden)?;
    assert_eq!(
        view.verify(0, MAX_FRAME_SIZE),
        Err(ProtocolError(ProtocolErrorEnum::WrongCluster {
            expected: 0,
            found: CLUSTER
        }))
    );
    Ok(())
}

#[test]
fn test_a_flag_bit_other_than_last_is_refused_naming_the_flags() -> TestResult {
    for flags in 2u8..=255 {
        let bytes = resealed(|b| b[65] = flags)?;
        let want = Err(ProtocolErrorEnum::FieldValue {
            command: 1,
            field: "flags",
            value: flags.into(),
        });
        assert_eq!(verify(&bytes)?, want, "flags {flags:#04x}");
        assert_eq!(verify_client(&bytes)?, want, "flags {flags:#04x}");
    }
    Ok(())
}

#[test]
fn test_a_clients_frame_has_view_and_replica_zero() -> TestResult {
    let bytes = resealed(|b| put(b, 44, 7, 4))?;
    assert_eq!(
        verify_client(&bytes)?,
        Err(ProtocolErrorEnum::FieldValue {
            command: 1,
            field: "view",
            value: 7
        })
    );
    assert_eq!(
        verify(&bytes)?.map(|h| h.view()),
        Ok(7),
        "a server's view is free"
    );
    let replica_set = resealed(|b| b[64] = 3)?;
    assert_eq!(
        verify_client(&replica_set)?,
        Err(ProtocolErrorEnum::FieldValue {
            command: 1,
            field: "replica",
            value: 3
        })
    );
    assert_eq!(verify(&replica_set)?.map(|h| h.replica()), Ok(3));
    // The view is named before the replica, and the flags before both.
    let view_and_replica = resealed(|b| {
        put(b, 44, 7, 4);
        b[64] = 3;
    })?;
    assert!(matches!(
        verify_client(&view_and_replica)?,
        Err(ProtocolErrorEnum::FieldValue { field: "view", .. })
    ));
    let view_and_flags = resealed(|b| {
        put(b, 44, 7, 4);
        b[65] = 0x02;
    })?;
    assert!(matches!(
        verify_client(&view_and_flags)?,
        Err(ProtocolErrorEnum::FieldValue { field: "flags", .. })
    ));
    assert!(verify_client(&HELLO.bytes()?)?.is_ok());
    Ok(())
}

#[test]
fn test_size_is_at_least_a_header_and_at_most_the_announced_maximum() -> TestResult {
    for size in [0u32, 1, 255] {
        let bytes = resealed(|b| put(b, 40, size.into(), 4))?;
        assert_eq!(
            verify(&bytes)?,
            Err(ProtocolErrorEnum::SizeBelowHeader { size })
        );
    }
    for size in [256u32, 4_096, 1_048_576] {
        let bytes = resealed(|b| put(b, 40, size.into(), 4))?;
        assert_eq!(
            verify(&bytes)?.map(|h| h.command()),
            Ok(Command::Hello),
            "size {size}"
        );
    }
    for size in [1_048_577u32, 2_097_152, u32::MAX] {
        let bytes = resealed(|b| put(b, 40, size.into(), 4))?;
        assert_eq!(
            verify(&bytes)?,
            Err(ProtocolErrorEnum::FrameTooLarge {
                size: size as usize,
                limit: 1_048_576
            })
        );
    }
    // A receiver that announced 524,288 bytes takes a frame of that size and refuses one byte more.
    let at = |size: u32, max_frame: usize| -> TestResult<Result<(), ProtocolErrorEnum>> {
        let bytes = resealed(|b| put(b, 40, size.into(), 4))?;
        Ok(HeaderView::new(&bytes)?
            .verify(CLUSTER, max_frame)
            .map(|_| ())
            .map_err(|e| e.0))
    };
    assert_eq!(at(524_288, 524_288)?, Ok(()));
    assert_eq!(
        at(524_289, 524_288)?,
        Err(ProtocolErrorEnum::FrameTooLarge {
            size: 524_289,
            limit: 524_288
        })
    );
    // An announced maximum above 1 MiB is held to the protocol's.
    assert_eq!(
        at(1_048_577, 2 * MAX_FRAME_SIZE)?,
        Err(ProtocolErrorEnum::FrameTooLarge {
            size: 1_048_577,
            limit: 1_048_576
        })
    );
    Ok(())
}

#[test]
fn test_rules_are_checked_in_the_protocols_order() -> TestResult {
    // Each step breaks one more rule earlier in the order; the earliest broken is the one reported: checksum,
    // command, reserved bytes, cluster, flags, view and replica of a client's frame, size.
    let bytes = resealed(|b| put(b, 40, u32::MAX.into(), 4))?;
    assert!(matches!(
        verify_client(&bytes)?,
        Err(ProtocolErrorEnum::FrameTooLarge { .. })
    ));
    let oversized_replicated = resealed(|b| {
        put(b, 40, u32::MAX.into(), 4);
        b[64] = 1;
    })?;
    assert!(matches!(
        verify_client(&oversized_replicated)?,
        Err(ProtocolErrorEnum::FieldValue {
            field: "replica",
            ..
        })
    ));
    let small_flagged_replicated = resealed(|b| {
        put(b, 40, 10, 4);
        b[64] = 1;
        b[65] = 0x80;
    })?;
    assert!(matches!(
        verify_client(&small_flagged_replicated)?,
        Err(ProtocolErrorEnum::FieldValue { field: "flags", .. })
    ));
    let all_broken = |b: &mut Vec<u8>| {
        put(b, 40, u32::MAX.into(), 4);
        b[65] = 0x80;
        put(b, 32, 7, 8);
        b[66] = 1;
        put(b, 54, 999, 2);
    };
    let small_flagged_foreign = resealed(|b| {
        put(b, 40, 10, 4);
        b[65] = 0x80;
        put(b, 32, 7, 8);
    })?;
    assert!(matches!(
        verify_client(&small_flagged_foreign)?,
        Err(ProtocolErrorEnum::WrongCluster { .. })
    ));
    let oversized_foreign_reserved = resealed(|b| {
        put(b, 40, u32::MAX.into(), 4);
        put(b, 32, 7, 8);
        b[66] = 1;
    })?;
    assert!(matches!(
        verify_client(&oversized_foreign_reserved)?,
        Err(ProtocolErrorEnum::ReservedNotZero { .. })
    ));
    let all_broken_frame = resealed(all_broken)?;
    assert_eq!(
        verify_client(&all_broken_frame)?,
        Err(ProtocolErrorEnum::UnknownCommand { command: 999 })
    );
    let mut all_broken_unsealed = resealed(all_broken)?;
    all_broken_unsealed[0] ^= 1;
    assert!(matches!(
        verify_client(&all_broken_unsealed)?,
        Err(ProtocolErrorEnum::HeaderChecksum { .. })
    ));
    Ok(())
}
