/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_log::LogError;
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    ChunkedBody, Command, HEADER_SIZE, HeaderView, MAX_FRAME_SIZE, RoundHeader,
};
use ultragraphdb_traits_log::Position;

use crate::types::damage::Damage;
use crate::types::scan_start::ScanStart;
use crate::types::scanned_round::ScannedRound;
use crate::types::segment_scan::SegmentScan;
use crate::utils::block::BLOCK;

/// What a block boundary of a segment holds.
enum Found {
    /// A whole block of zeros: preallocated, never written.
    Zeros,
    /// Something that is not a Round frame of this cluster fitting in the segment.
    NotARound,
    /// A Round frame of this cluster whose header verifies, its body not yet checked.
    Frame {
        round: ScannedRound,
        incarnation: u64,
        checksum_body: u64,
    },
}

/// The block at `offset` read as a frame, in the protocol's order: its `size` bounded by the frame maximum and the
/// rest of `bytes` before the frame is sliced, then the header's checksum, command, reserved bytes and cluster.
fn frame_at(bytes: &[u8], offset: usize, cluster: u64) -> Found {
    let Some(block) = offset
        .checked_add(BLOCK)
        .and_then(|end| bytes.get(offset..end))
    else {
        return Found::NotARound;
    };
    if block.iter().all(|byte| *byte == 0) {
        return Found::Zeros;
    }
    let size = block
        .get(40..44)
        .and_then(|field| field.try_into().ok())
        .map_or(0, u32::from_le_bytes) as usize;
    if !(HEADER_SIZE..=MAX_FRAME_SIZE).contains(&size) || size > bytes.len() - offset {
        return Found::NotARound;
    }
    let frame = bytes.get(offset..offset + size).unwrap_or_default();
    // size is at least a header, so the view's length rule holds; verify checks the rest.
    let verified =
        HeaderView::new(frame).and_then(|view| Ok((view, view.verify(cluster, MAX_FRAME_SIZE)?)));
    match verified {
        Ok((view, header)) if header.command() == Command::Round => {
            let fields = RoundHeader::decode(view.command_header());
            // A Round frame is numbered by its round's position: a header saying otherwise is no round of the log.
            if header.sequence() != u128::from(fields.position()) {
                return Found::NotARound;
            }
            Found::Frame {
                round: ScannedRound::new(
                    Position::new(fields.position()),
                    fields.parent(),
                    view.checksum(),
                    fields.first_version(),
                    fields.transactions(),
                    offset as u64,
                    size as u32,
                ),
                incarnation: fields.incarnation(),
                checksum_body: view.checksum_body(),
            }
        }
        _ => Found::NotARound,
    }
}

/// The body of `round`, whose header verified, checked against `checksum_body`: its chunk table, then each chunk.
fn check_body(bytes: &[u8], round: &ScannedRound, checksum_body: u64) -> Result<(), Damage> {
    let start = round.offset() as usize + HEADER_SIZE;
    let end = round.offset() as usize + round.size() as usize;
    let body = bytes.get(start..end).unwrap_or_default();
    ChunkedBody::verify(body, checksum_body)
        .map(|_| ())
        .map_err(|refusal| match refusal.0 {
            ProtocolErrorEnum::Chunk { index, start, end } => Damage::Chunk {
                position: round.position(),
                index,
                start,
                end,
            },
            // The table failed checksum_body, or the body's length has no table to fail.
            _ => Damage::ChunkTable {
                position: round.position(),
            },
        })
}

/// What a scan does with a block.
enum Step {
    Round(ScannedRound),
    Clean,
    Damaged(Damage),
}

/// The block at `offset` against the round expected there: `next`, or for the first segment kept any round at or
/// below the one after the checkpoint.
fn step(
    bytes: &[u8],
    offset: usize,
    start: &ScanStart,
    next: Option<(Position, u64)>,
) -> Result<Step, LogError> {
    let (round, checksum_body) = match frame_at(bytes, offset, start.cluster()) {
        Found::Zeros => return Ok(Step::Clean),
        Found::NotARound => return Ok(Step::Damaged(Damage::Header)),
        Found::Frame { incarnation, .. } if incarnation != start.incarnation() => {
            return Ok(Step::Clean);
        }
        Found::Frame {
            round,
            checksum_body,
            ..
        } => (round, checksum_body),
    };
    match next {
        Some((position, parent)) => {
            if round.position() != position || round.parent() != parent {
                return Ok(Step::Damaged(Damage::Chain {
                    position: round.position(),
                }));
            }
        }
        None => check_first(&round, start)?,
    }
    if let Err(damage) = check_body(bytes, &round, checksum_body) {
        return Ok(Step::Damaged(damage));
    }
    if start.checkpoint() > Position::ZERO
        && round.position() == start.checkpoint()
        && round.checksum() != start.checkpoint_checksum()
    {
        return Err(LogError::Diverged(
            "the checkpoint's checksum",
            start.checkpoint_checksum(),
            round.checksum(),
        ));
    }
    Ok(Step::Round(round))
}

/// The first round of the first segment kept lies at or below the round after the checkpoint, and is that round only
/// with the checkpoint's checksum as its parent.
fn check_first(round: &ScannedRound, start: &ScanStart) -> Result<(), LogError> {
    let after = start.checkpoint().get().saturating_add(1);
    let position = round.position().get();
    if position > after {
        return Err(LogError::Diverged(
            "the first round's position",
            after,
            position,
        ));
    }
    if position == after && round.parent() != start.checkpoint_checksum() {
        return Err(LogError::Diverged(
            "the first round's parent",
            start.checkpoint_checksum(),
            round.parent(),
        ));
    }
    Ok(())
}

/// The rounds of one segment's bytes, from its first block, in the order the log checks a round: its `size` bounded
/// by the frame maximum and the rest of the segment before anything is read past the block; then its header
/// checksum, command, reserved bytes and cluster; then its incarnation and its place in the chain; then its chunk
/// table and each chunk. The scan stops at the first block that is not the next round of this incarnation: a block
/// of zeros or a valid round of another incarnation is a clean end; anything else is damage, named in the result.
/// Trailing bytes short of a block are not read. A round whose frame fits in the bytes is accepted though its padding
/// does not, and the scan then ends at its padded end, past the bytes.
///
/// Refused as [`LogError::Diverged`] when the chain contradicts the checkpoint: a round at the checkpoint whose
/// checksum is not the checkpoint's, or a first segment whose first round lies past the round after the checkpoint
/// or is that round with another parent.
pub fn scan_segment(bytes: &[u8], start: &ScanStart) -> Result<SegmentScan, LogError> {
    let mut rounds = Vec::new();
    let mut offset = 0usize;
    let mut next = start.next();
    loop {
        if bytes.len().saturating_sub(offset) < BLOCK {
            return Ok(SegmentScan::new(rounds, offset as u64, None));
        }
        match step(bytes, offset, start, next)? {
            Step::Round(round) => {
                next = round
                    .position()
                    .next()
                    .map(|position| (position, round.checksum()));
                offset += round.padded_size() as usize;
                rounds.push(round);
                if next.is_none() {
                    return Ok(SegmentScan::new(rounds, offset as u64, None));
                }
            }
            Step::Clean => return Ok(SegmentScan::new(rounds, offset as u64, None)),
            Step::Damaged(damage) => {
                return Ok(SegmentScan::new(rounds, offset as u64, Some(damage)));
            }
        }
    }
}

/// The offset of the first complete round of `cluster` and `incarnation` at a block boundary at or after `from` in
/// `bytes`, at `min_position` or later: a frame whose header verifies, whose chunks all match, and which fits in the
/// bytes. `None` when there is none. A recovery asks this after damage: a complete round past it means the damage
/// is not a torn tail.
pub fn find_complete_round(
    bytes: &[u8],
    from: usize,
    cluster: u64,
    incarnation: u64,
    min_position: Position,
) -> Option<usize> {
    let first = from.checked_next_multiple_of(BLOCK)?;
    (first..bytes.len()).step_by(BLOCK).find(|&offset| {
        matches!(
            frame_at(bytes, offset, cluster),
            Found::Frame { round, incarnation: found, checksum_body }
                if found == incarnation
                    && round.position() >= min_position
                    && check_body(bytes, &round, checksum_body).is_ok()
        )
    })
}
