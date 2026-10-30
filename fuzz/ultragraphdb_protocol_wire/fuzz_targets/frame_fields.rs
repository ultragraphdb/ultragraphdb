/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Structure-aware: a valid frame built from arbitrary fields and body, which must validate and read back; then header
//! rules broken on purpose and the header resealed, so the rules after the checksum are reached and the refusal is
//! the first rule broken in the protocol's order (PROTOCOL.md section 2.5, steps 3 to 9); then arbitrary byte
//! mutations, which never panic. The sequence is any: a connection's receiver checks it, not the frame's validation.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{Command, FLAG_LAST, FrameView, Header, MAX_FRAME_SIZE, xxh3_64};

const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

#[derive(Debug, Arbitrary)]
enum Mutation {
    Flip { at: u32, bit: u8 },
    Truncate { len: u32 },
    Append { byte: u8 },
    SwapChunks { a: u8, b: u8 },
    Reseal,
}

#[derive(Debug, Arbitrary)]
struct Input {
    command: u8,
    sequence: u128,
    view: u32,
    release: u32,
    protocol: u16,
    stream: u64,
    replica: u8,
    flags: u8,
    command_header: [u8; 128],
    body: Vec<u8>,
    /// A command number written over the valid one.
    command_number: Option<u16>,
    /// A reserved byte, `offset % 62` into 66..128, set to a value.
    reserved: Option<(u8, u8)>,
    /// Another cluster written over the frame's.
    cluster: Option<u64>,
    /// Flags written over the frame's, bit 0 alone allowed.
    flags_written: Option<u8>,
    /// Another size written over the frame's.
    size: Option<u32>,
    mutations: Vec<Mutation>,
}

fn reseal(frame: &mut [u8]) {
    if let Some(tail) = frame.get(8..256) {
        let checksum = xxh3_64(tail).to_le_bytes();
        if let Some(head) = frame.get_mut(0..8) {
            head.copy_from_slice(&checksum);
        }
    }
}

fn put(frame: &mut [u8], at: usize, bytes: &[u8]) {
    if let Some(slot) = frame.get_mut(at..at + bytes.len()) {
        slot.copy_from_slice(bytes);
    }
}

fuzz_target!(|input: Input| {
    let command = Command::ALL[usize::from(input.command) % Command::ALL.len()];
    let header = Header::new(CLUSTER, command)
        .with_sequence(input.sequence)
        .with_view(input.view)
        .with_release(input.release)
        .with_protocol(input.protocol)
        .with_stream(input.stream)
        .with_replica(input.replica)
        .with_flags(input.flags & FLAG_LAST)
        .with_command_header(input.command_header);
    let Ok(valid) = header.encode_frame(&input.body) else {
        assert!(input.body.len() > MAX_FRAME_SIZE - 256 - 2_048);
        return;
    };
    let frame =
        FrameView::validate(&valid, CLUSTER, MAX_FRAME_SIZE).expect("a built frame validates");
    assert_eq!(frame.body(), &input.body[..]);
    assert_eq!(
        frame.header().verify(CLUSTER, MAX_FRAME_SIZE).as_ref(),
        Ok(&header)
    );

    // Break header rules, reseal, and predict the first rule broken.
    let mut broken = valid.clone();
    let mut predicted: Option<ProtocolErrorEnum> = None;
    let mut predict = |e: ProtocolErrorEnum| {
        predicted.get_or_insert(e);
    };
    if let Some(number) = input.command_number {
        put(&mut broken, 54, &number.to_le_bytes());
        if Command::from_number(number).is_err() {
            predict(ProtocolErrorEnum::UnknownCommand { command: number });
        }
    }
    if let Some((offset, value)) = input.reserved {
        let at = 66 + usize::from(offset) % 62;
        put(&mut broken, at, &[value]);
        if value != 0 {
            predict(ProtocolErrorEnum::ReservedNotZero { offset: at });
        }
    }
    if let Some(cluster) = input.cluster {
        put(&mut broken, 32, &cluster.to_le_bytes());
        if cluster != CLUSTER {
            predict(ProtocolErrorEnum::WrongCluster {
                expected: CLUSTER,
                found: cluster,
            });
        }
    }
    if let Some(flags) = input.flags_written {
        put(&mut broken, 65, &[flags]);
        if flags & !FLAG_LAST != 0 {
            let number = u16::from_le_bytes([broken[54], broken[55]]);
            predict(ProtocolErrorEnum::FieldValue {
                command: number,
                field: "flags",
                value: flags.into(),
            });
        }
    }
    if let Some(size) = input.size {
        put(&mut broken, 40, &size.to_le_bytes());
        let len = broken.len();
        if (size as usize) < 256 {
            predict(ProtocolErrorEnum::SizeBelowHeader { size });
        } else if size as usize > MAX_FRAME_SIZE {
            predict(ProtocolErrorEnum::FrameTooLarge {
                size: size as usize,
                limit: MAX_FRAME_SIZE,
            });
        } else if size as usize != len {
            predict(ProtocolErrorEnum::LengthMismatch { size, len });
        }
    }
    reseal(&mut broken);
    match (
        FrameView::validate(&broken, CLUSTER, MAX_FRAME_SIZE),
        predicted,
    ) {
        (Ok(_), None) => {}
        (Err(e), Some(want)) => assert_eq!(e.0, want),
        (got, want) => panic!("validated as {got:?}, predicted {want:?}"),
    }

    // Arbitrary mutations of the valid frame: no panic, and an unchanged frame still validates.
    let mut mutated = valid.clone();
    for mutation in input.mutations.iter().take(16) {
        match *mutation {
            Mutation::Flip { at, bit } => {
                if let Some(byte) = mutated.get_mut(at as usize % (valid.len() + 1)) {
                    *byte ^= 1 << (bit % 8);
                }
            }
            Mutation::Truncate { len } => mutated.truncate(len as usize),
            Mutation::Append { byte } => mutated.push(byte),
            Mutation::SwapChunks { a, b } => {
                let range = |i: u8| {
                    let start = 256 + 3_840 + 4_096 * usize::from(i % 4);
                    start..start + 4_096
                };
                let (a, b) = (range(a), range(b));
                if a != b && mutated.len() >= a.end.max(b.end) {
                    let chunk_a = mutated[a.clone()].to_vec();
                    let chunk_b = mutated[b.clone()].to_vec();
                    mutated[a].copy_from_slice(&chunk_b);
                    mutated[b].copy_from_slice(&chunk_a);
                }
            }
            Mutation::Reseal => reseal(&mut mutated),
        }
    }
    let outcome = FrameView::validate(&mutated, CLUSTER, MAX_FRAME_SIZE);
    if mutated == valid {
        assert!(outcome.is_ok());
    }
});
