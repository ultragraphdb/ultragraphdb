/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The wire-protocol scenarios of the header, checksums and validation, and, in `bodies_steps`, of the bodies.

use cucumber::{World, then, when};
use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};
use ultragraphdb_protocol_wire::{
    Command, FrameView, Header, HeaderView, RETIRED_COMMANDS, xxh3_64, xxh3_64_with_seed,
};

// The golden frames of the unit tests, shared by path.
mod utils {
    #[allow(dead_code)]
    #[path = "golden.rs"]
    pub mod golden;
    #[allow(dead_code)]
    #[path = "golden_bodies.rs"]
    pub mod golden_bodies;
}

mod bodies_steps;

use crate::utils::golden::{CLUSTER, CREDIT, GOLDEN, HELLO, READ, put, reseal};

/// The cluster a receiver in these scenarios belongs to: the golden frames'.
const SERVER: u64 = CLUSTER;

#[derive(Debug, Default, World)]
pub struct Steps {
    bytes: Vec<u8>,
    frames: Vec<Vec<u8>>,
    refusals: Vec<ProtocolError>,
    checksums: Vec<(u64, u64)>,
    statement: Option<Result<(), ultragraphdb_protocol_wire::StatementError>>,
    statement_again: Option<Result<(), ultragraphdb_protocol_wire::StatementError>>,
    schemas: Vec<u64>,
    statements: Vec<u64>,
    truncations: usize,
}

// One header for every frame.

#[when("the golden Hello frame's header is read")]
fn read_hello(w: &mut Steps) {
    w.bytes = HELLO.bytes();
}

#[then("each field holds its golden value, and writing the fields back yields the same 256 bytes")]
fn hello_round_trips(w: &mut Steps) {
    let view = HeaderView::new(&w.bytes).expect("a header");
    // generate_frames.c's Hello: cluster, release 0x00050000, protocol 1, command 1, the rest zero, command header
    // byte i = i * 7 + 3, a 100-byte body.
    assert_eq!(view.checksum(), 0x8B60_6B87_756C_942F);
    assert_eq!(view.checksum_body(), 0x93CD_9543_2B7D_483F);
    assert_eq!(view.nonce_reserved(), 0);
    assert_eq!(view.cluster(), 0x0123_4567_89AB_CDEF);
    assert_eq!(view.size(), 356);
    assert_eq!(view.view(), 0);
    assert_eq!(view.release(), 0x0005_0000);
    assert_eq!(view.protocol(), 1);
    assert_eq!(view.command(), 1);
    assert_eq!(view.stream(), 0);
    assert_eq!(view.replica(), 0);
    assert_eq!(view.flags(), 0);
    assert_eq!(view.reserved(), &[0u8; 62][..]);
    assert_eq!(view.command_header(), &HELLO.command_header());
    let header = view.verify(SERVER).expect("the golden header verifies");
    assert_eq!(header.command(), Command::Hello);
    assert_eq!(
        header.encode(view.size(), view.checksum_body()).as_slice(),
        &w.bytes[..256]
    );
}

// XXH3-64 checksums.

#[when(
    "a header's checksum is computed by this crate and by the xxHash reference implementation over the same bytes"
)]
fn checksum_both_ways(w: &mut Steps) {
    // The reference's value is the one generate_frames.c stored at offset 0, computed by xxhash.h v0.8.3.
    w.checksums = GOLDEN
        .iter()
        .map(|golden| {
            let bytes = golden.bytes();
            (xxh3_64(&bytes[8..256]), golden.checksum)
        })
        .collect();
}

#[then("both give the same 64-bit value")]
fn same_checksum(w: &mut Steps) {
    assert_eq!(w.checksums.len(), 6);
    for (ours, reference) in &w.checksums {
        assert_eq!(ours, reference);
    }
}

// Chunked body checksums.

#[when("one byte of chunk 3 of a 20 KiB body is flipped")]
fn flip_chunk_three(w: &mut Steps) {
    // READ's body is 20,480 bytes: 20,432 of chunks and a 48-byte table. Chunk 3 is body bytes 12,032 to 16,128.
    let mut bytes = READ.bytes();
    assert_eq!(bytes.len() - 256, 20 * 1_024);
    bytes[256 + 14_000] ^= 0x01;
    w.refusals = FrameView::validate(&bytes, SERVER)
        .err()
        .into_iter()
        .collect();
}

#[then("the frame is refused with an error naming chunk 3 and its byte range")]
fn chunk_three_named(w: &mut Steps) {
    assert_eq!(
        w.refusals,
        vec![ProtocolError(ProtocolErrorEnum::Chunk {
            index: 3,
            start: 12_032,
            end: 16_128
        })]
    );
    assert!(w.refusals[0].to_string().contains("chunk 3"));
    assert!(w.refusals[0].to_string().contains("12032..16128"));
}

#[when("chunks 1 and 2 of a body are swapped")]
fn swap_chunks(w: &mut Steps) {
    let mut bytes = READ.bytes();
    let (one, two) = (256 + 3_840..256 + 7_936, 256 + 7_936..256 + 12_032);
    let chunk_one = bytes[one.clone()].to_vec();
    let chunk_two = bytes[two.clone()].to_vec();
    bytes[one].copy_from_slice(&chunk_two);
    bytes[two].copy_from_slice(&chunk_one);
    w.refusals = FrameView::validate(&bytes, SERVER)
        .err()
        .into_iter()
        .collect();
}

#[then("the frame is refused naming chunk 1")]
fn chunk_one_named(w: &mut Steps) {
    assert_eq!(
        w.refusals,
        vec![ProtocolError(ProtocolErrorEnum::Chunk {
            index: 1,
            start: 3_840,
            end: 7_936
        })]
    );
}

#[when("a frame with a 100-byte body is built")]
fn build_small(w: &mut Steps) {
    let header = Header::new(CLUSTER, Command::Hello)
        .with_release(0x0005_0000)
        .with_command_header(HELLO.command_header());
    w.bytes = header.encode_frame(&HELLO.data()).expect("a small frame");
}

#[then("its body is 100 bytes, and `checksum_body` is the leaf of that one chunk")]
fn small_has_no_table(w: &mut Steps) {
    assert_eq!(w.bytes.len(), 356);
    assert_eq!(w.bytes, HELLO.bytes(), "the reference generator's frame");
    let view = HeaderView::new(&w.bytes).unwrap();
    // The leaf of chunk 0: XXH3-64 of the body with seed 0, by the reference implementation.
    assert_eq!(view.checksum_body(), 0x93CD_9543_2B7D_483F);
    assert_eq!(view.checksum_body(), xxh3_64_with_seed(&w.bytes[256..], 0));
}

#[when(
    "a frame arrives whose body is 3,841 bytes, one byte past the first chunk with no room for a table"
)]
fn arrives_3841(w: &mut Steps) {
    let mut bytes = HELLO.bytes()[..256].to_vec();
    bytes.extend_from_slice(&[0x3C; 3_841]);
    put(&mut bytes, 40, 256 + 3_841, 4);
    reseal(&mut bytes);
    w.refusals = FrameView::validate(&bytes, SERVER)
        .err()
        .into_iter()
        .collect();
}

#[then("it is refused with an error naming the body's length")]
fn body_length_named(w: &mut Steps) {
    assert_eq!(
        w.refusals,
        vec![ProtocolError(ProtocolErrorEnum::BodyLength { len: 3_841 })]
    );
    assert!(w.refusals[0].to_string().contains("3841"));
}

// Frames are validated before use and never panic.

#[when("a frame whose `cluster` is not the server's arrives")]
fn other_cluster(w: &mut Steps) {
    let mut bytes = CREDIT.bytes();
    put(&mut bytes, 32, 0xFEDC_BA98_7654_3210, 8);
    reseal(&mut bytes);
    w.refusals = FrameView::validate(&bytes, SERVER)
        .err()
        .into_iter()
        .collect();
}

#[then("the server answers with an error naming the cluster and closes the connection")]
fn cluster_named(w: &mut Steps) {
    // The decision is this crate's: a ProtocolError, which the server answers and closes on (group 10).
    assert_eq!(
        w.refusals,
        vec![ProtocolError(ProtocolErrorEnum::WrongCluster {
            expected: SERVER,
            found: 0xFEDC_BA98_7654_3210
        })]
    );
    assert!(w.refusals[0].to_string().contains("0xfedcba9876543210"));
}

#[when("a frame with a valid checksum names an unknown command")]
fn unknown_command(w: &mut Steps) {
    let mut bytes = CREDIT.bytes();
    put(&mut bytes, 54, 18, 2);
    reseal(&mut bytes);
    assert!(HeaderView::new(&bytes).unwrap().checksum() == xxh3_64(&bytes[8..256]));
    w.refusals = FrameView::validate(&bytes, SERVER)
        .err()
        .into_iter()
        .collect();
    // Another connection's frame, validated after the refusal.
    w.frames = vec![READ.bytes()];
}

#[then(
    "the connection is closed with an error naming the command, and the server keeps serving other connections"
)]
fn command_named(w: &mut Steps) {
    assert_eq!(
        w.refusals,
        vec![ProtocolError(ProtocolErrorEnum::UnknownCommand {
            command: 18
        })]
    );
    assert_eq!(w.refusals[0].to_string(), "unknown command 18");
    let other = FrameView::validate(&w.frames[0], SERVER).expect("another frame is served");
    assert_eq!(other.command(), Command::Read);
}

// Commands keep their numbers.

#[when("a frame carries a retired command's number")]
fn retired_number(w: &mut Steps) {
    // No command is retired yet: RETIRED_COMMANDS is empty, and retiring one removes its variant, so a retired
    // number is a number outside the registry. Every such number is sent here, the retired ones among them.
    let numbers: Vec<u16> = RETIRED_COMMANDS
        .iter()
        .copied()
        .chain((0..=u16::MAX).filter(|n| !(1..=17).contains(n)))
        .collect();
    let golden = CREDIT.bytes();
    w.refusals = numbers
        .iter()
        .map(|&number| {
            let mut bytes = golden.clone();
            put(&mut bytes, 54, number.into(), 2);
            reseal(&mut bytes);
            FrameView::validate(&bytes, SERVER).expect_err("an unregistered number is refused")
        })
        .collect();
    w.checksums = numbers.iter().map(|&n| (n.into(), 0)).collect();
}

#[then("it is refused as an unknown command")]
fn refused_unknown(w: &mut Steps) {
    assert_eq!(w.refusals.len(), 65_536 - 17);
    for (refusal, (number, _)) in w.refusals.iter().zip(&w.checksums) {
        assert_eq!(
            refusal.0,
            ProtocolErrorEnum::UnknownCommand {
                command: *number as u16
            }
        );
    }
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()),
    );
}
