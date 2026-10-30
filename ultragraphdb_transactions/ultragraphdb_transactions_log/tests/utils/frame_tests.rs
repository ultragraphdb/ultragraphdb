/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_log::LogErrorEnum;
use ultragraphdb_protocol_wire::{Command, FrameView, MAX_FRAME_SIZE};
use ultragraphdb_traits_log::{Position, RoundRef};
use ultragraphdb_transactions_log::encode_round;

use crate::utils::helpers::{CLUSTER, body, frame_checksum};

fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().expect("8 bytes"))
}

#[test]
fn test_a_round_is_a_round_frame_laid_out_by_the_tables() {
    let data = body(3, 1_000);
    let round = RoundRef::new(Position::new(3), 0xA1B2_C3D4_E5F6_0718, 40, 5, &data);
    let (frame, checksum) = encode_round(CLUSTER, 0, 2, &round).expect("encoded");
    let bytes = frame.as_slice();
    // 256 + 1,000 bytes, one chunk and no table, padded to one block.
    assert_eq!(bytes.len(), 4_096);
    assert_eq!(u64_at(bytes, 32), CLUSTER);
    assert_eq!(
        u32::from_le_bytes(bytes[40..44].try_into().expect("4")),
        1_256
    );
    assert_eq!(
        u16::from_le_bytes([bytes[54], bytes[55]]),
        17,
        "Round is command 17"
    );
    // The command header: position, parent, incarnation, first version, transactions, then zeros.
    assert_eq!(u64_at(bytes, 128), 3);
    assert_eq!(u64_at(bytes, 136), 0xA1B2_C3D4_E5F6_0718);
    assert_eq!(u64_at(bytes, 144), 2);
    assert_eq!(u64_at(bytes, 152), 40);
    assert_eq!(
        u32::from_le_bytes(bytes[160..164].try_into().expect("4")),
        5
    );
    assert!(bytes[164..256].iter().all(|b| *b == 0));
    assert_eq!(&bytes[256..1_256], data.as_slice());
    assert!(bytes[1_256..].iter().all(|b| *b == 0), "padding is zeros");
    assert_eq!(checksum, frame_checksum(bytes));
    assert_eq!(checksum, u64_at(bytes, 0));
    let view =
        FrameView::validate(&bytes[..1_256], CLUSTER, MAX_FRAME_SIZE).expect("a valid frame");
    assert_eq!(view.command(), Command::Round);
    assert_eq!(view.body(), data.as_slice());
}

#[test]
fn test_a_round_of_two_chunks_carries_its_table_and_pads_to_two_blocks() {
    let data = body(4, 5_000);
    let round = RoundRef::new(Position::new(4), 1, 2, 3, &data);
    let (frame, _) = encode_round(CLUSTER, 0, 1, &round).expect("encoded");
    // 256 + 5,000 + a 16-byte table = 5,272 bytes, two blocks.
    assert_eq!(frame.len(), 8_192);
    let view = FrameView::validate(&frame.as_slice()[..5_272], CLUSTER, MAX_FRAME_SIZE)
        .expect("a valid frame");
    assert_eq!(view.body(), data.as_slice());
}

#[test]
fn test_an_empty_body_is_one_block() {
    let round = RoundRef::new(Position::new(1), 0, 0, 0, &[]);
    let (frame, checksum) = encode_round(CLUSTER, 0, 1, &round).expect("encoded");
    assert_eq!(frame.len(), 4_096);
    assert_eq!(checksum, frame_checksum(frame.as_slice()));
}

#[test]
fn test_the_largest_body_fits_and_one_byte_more_is_refused() {
    // 1,046,272 bytes of data take 256 chunks and a 2,048-byte table: exactly 1 MiB with the header.
    let largest = vec![7u8; 1_046_272];
    let (frame, _) = encode_round(
        CLUSTER,
        0,
        1,
        &RoundRef::new(Position::new(1), 0, 0, 0, &largest),
    )
    .expect("the largest round");
    assert_eq!(frame.len(), 1_048_576);
    let past = vec![7u8; 1_046_273];
    let error = encode_round(
        CLUSTER,
        0,
        1,
        &RoundRef::new(Position::new(1), 0, 0, 0, &past),
    )
    .expect_err("past the frame");
    assert_eq!(
        error.0,
        LogErrorEnum::TooLarge {
            size: 1_048_577,
            limit: 1_048_576
        }
    );
}

#[test]
fn test_the_incarnation_and_the_cluster_change_the_identity() {
    let data = body(1, 10);
    let round = RoundRef::new(Position::new(1), 0, 0, 0, &data);
    let one = encode_round(CLUSTER, 0, 1, &round).expect("encoded").1;
    let two = encode_round(CLUSTER, 0, 2, &round).expect("encoded").1;
    let other = encode_round(CLUSTER + 1, 0, 1, &round).expect("encoded").1;
    assert_ne!(one, two);
    assert_ne!(one, other);
}

/// The golden Round frame of `ultragraphdb_protocol_wire`, `testdata/golden/bodies/round.frame`.
fn golden_round() -> Vec<u8> {
    // The repository's root, three levels above this crate's `testdata/`, taken lexically: the directory need not exist.
    let testdata = ultragraphdb_tool_bdd::testdata_dir();
    let root = testdata.ancestors().nth(3).expect("the repository's root");
    let path = root.join(
        "ultragraphdb_protocol/ultragraphdb_protocol_wire/testdata/golden/bodies/round.frame",
    );
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A round the log writes is the protocol's Round frame byte for byte: its header numbered by its position and naming
/// the writer's release, its command header and its chunked body, as the golden frame generated from `PROTOCOL.md`
/// alone is (§12, point 16).
#[test]
fn test_a_round_is_the_golden_round_frame_byte_for_byte() {
    use ultragraphdb_protocol_wire::RoundHeader;
    let golden = golden_round();
    let view = FrameView::validate(&golden, u64_at(&golden, 32), MAX_FRAME_SIZE)
        .expect("the golden frame");
    let header = view.header();
    let fields = RoundHeader::decode(header.command_header());
    assert_eq!(header.sequence(), 3, "the golden round's position");
    let round = RoundRef::new(
        Position::new(fields.position()),
        fields.parent(),
        fields.first_version(),
        fields.transactions(),
        view.chunked_body().data(),
    );
    let (frame, checksum) = encode_round(
        header.cluster(),
        header.release(),
        fields.incarnation(),
        &round,
    )
    .expect("encoded");
    assert_eq!(&frame.as_slice()[..golden.len()], golden.as_slice());
    assert!(
        frame.as_slice()[golden.len()..].iter().all(|b| *b == 0),
        "padded with zeros"
    );
    assert_eq!(checksum, header.checksum());
}

/// A round's header is numbered by its position and names the release it was written by.
#[test]
fn test_a_rounds_header_names_its_position_and_its_writers_release() {
    use ultragraphdb_protocol_wire::HeaderView;
    let data = body(9, 100);
    let round = RoundRef::new(Position::new(9), 1, 2, 1, &data);
    let (frame, _) = encode_round(CLUSTER, 0x0006_0001, 1, &round).expect("encoded");
    let header = HeaderView::new(frame.as_slice()).unwrap();
    assert_eq!(
        (
            header.sequence(),
            header.release(),
            header.stream(),
            header.flags(),
            header.protocol()
        ),
        (9, 0x0006_0001, 0, 0, 1)
    );
}
