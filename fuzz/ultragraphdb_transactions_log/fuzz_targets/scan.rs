/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as a segment, scanned as the first segment kept of a log with its checkpoint at 0 and, from the same
//! bytes, at a checkpoint the input chooses. The scan never panics; every round it accepts is a frame that validates
//! on its own, at a block boundary, inside the segment, of this cluster and incarnation, numbered by its position from 1,
//! its command header the Round header of `PROTOCOL.md` §12.1 (a derivation version of at least 1, which the scanned
//! round carries, and the unused bytes 40 to 127 zero), and chained to the round before it; it stops at a block boundary less than a block past the bytes, inside the last round's padding when the
//! bytes end there; and the probe finds every accepted round complete.
//!
//! Random bytes almost never carry a frame's header checksum, so the input is scanned again with the header of each
//! frame it holds resealed, frame after frame from the first block as the scan walks them, which reaches the rules
//! behind the checksum: the cluster, the incarnation, the position and the chain.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_protocol_wire::{FrameView, HEADER_SIZE, MAX_FRAME_SIZE, RoundHeader, xxh3_64};
use ultragraphdb_traits_log::Position;
use ultragraphdb_transactions_log::{ScanStart, find_complete_round, scan_segment};

/// The seeds' cluster and incarnation, so seeds from `testdata/fuzz_seeds/scan/` reach the chain and the chunks.
const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;
const INCARNATION: u64 = 1;

fn check(bytes: &[u8], start: &ScanStart) {
    let Ok(scan) = scan_segment(bytes, start) else {
        return;
    };
    assert_eq!(scan.end() % 4_096, 0);
    assert!((scan.end() as usize) < bytes.len() + 4_096);
    let mut previous: Option<(u64, u64)> = start.next().map(|(p, parent)| (p.get() - 1, parent));
    let mut offset = 0u64;
    for round in scan.rounds() {
        assert_eq!(
            round.offset(),
            offset,
            "rounds lie end to end from the first block"
        );
        let at = round.offset() as usize;
        let frame = &bytes[at..at + round.size() as usize];
        let view = FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE)
            .expect("an accepted round's checksums hold");
        let command_header = view.header().command_header();
        let header =
            RoundHeader::decode(command_header).expect("an accepted round's header decodes");
        assert!(
            command_header[40..].iter().all(|byte| *byte == 0),
            "an accepted round's unused header bytes are zero"
        );
        assert_eq!(
            header.derivation(),
            round.derivation(),
            "the scanned round's derivation is its header's"
        );
        assert_eq!(header.first_version(), round.first_version());
        assert_eq!(header.transactions(), round.transactions());
        assert_eq!(header.incarnation(), INCARNATION);
        assert_eq!(header.position(), round.position().get());
        // Positions are gapless from 1 (PROTOCOL.md section 12.1): 0 is the place before the first round.
        assert!(
            round.position().get() >= 1,
            "an accepted round's position is at least 1"
        );
        assert_eq!(
            view.header().sequence(),
            u128::from(round.position().get()),
            "a round's sequence is its position"
        );
        assert_eq!(view.header().checksum(), round.checksum());
        if let Some((position, checksum)) = previous {
            assert_eq!(
                round.position().get(),
                position + 1,
                "positions are gapless"
            );
            assert_eq!(
                round.parent(),
                checksum,
                "each round names the one before it"
            );
        }
        assert_eq!(
            find_complete_round(bytes, at, CLUSTER, INCARNATION, round.position()),
            Some(at)
        );
        previous = Some((round.position().get(), round.checksum()));
        offset += round.padded_size();
    }
    assert!(scan.end() >= offset);
}

/// `bytes` with the header checksum of each frame resealed: at each block boundary from the first, a header whose size
/// is at least a header's is resealed and the walk moves past its frame's padding, otherwise to the next block.
fn resealed(bytes: &[u8]) -> Vec<u8> {
    let mut sealed = bytes.to_vec();
    let mut offset = 0usize;
    while let Some(tail) = sealed.get(offset + 8..offset + HEADER_SIZE) {
        let checksum = xxh3_64(tail).to_le_bytes();
        let size = u32::from_le_bytes(tail[32..36].try_into().expect("four bytes")) as usize;
        if size < HEADER_SIZE {
            offset += 4_096;
            continue;
        }
        sealed[offset..offset + 8].copy_from_slice(&checksum);
        offset += size.div_ceil(4_096) * 4_096;
    }
    sealed
}

fuzz_target!(|data: &[u8]| {
    let (checkpoint, segment) = data.split_at(data.len().min(8));
    let mut first = [0u8; 8];
    first[..checkpoint.len()].copy_from_slice(checkpoint);
    // The checkpoint's position from the first byte only, so it lies near the seeds' positions; its checksum 0.
    let at = Position::new(u64::from(first[0] % 8));
    check(
        data,
        &ScanStart::new(CLUSTER, INCARNATION, Position::ZERO, 0),
    );
    check(
        segment,
        &ScanStart::new(CLUSTER, INCARNATION, at, u64::from_le_bytes(first)),
    );
    check(
        segment,
        &ScanStart::new(CLUSTER, INCARNATION, Position::ZERO, 0).continuing(Position::new(1), 0),
    );
    let sealed = resealed(data);
    check(
        &sealed,
        &ScanStart::new(CLUSTER, INCARNATION, Position::ZERO, 0),
    );
    check(
        &sealed,
        &ScanStart::new(CLUSTER, INCARNATION, Position::ZERO, 0).continuing(Position::new(1), 0),
    );
});
