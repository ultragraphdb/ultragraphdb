/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::ops::RangeInclusive;

/// The header's length, in front of every frame: client frames, log records and replica frames.
pub const HEADER_SIZE: usize = 256;

/// The command header's length, the header's last 128 bytes, laid out by each command.
pub const COMMAND_HEADER_SIZE: usize = 128;

/// The largest frame, header and body: 1 MiB, a multiple of 4,096 so a log record can be written with direct I/O.
pub const MAX_FRAME_SIZE: usize = 1 << 20;

/// The body's first chunk: the rest of the frame's first 4,096-byte block after the header.
pub const FIRST_CHUNK_SIZE: usize = 3_840;

/// Every later chunk, one 4,096-byte block of the frame; the last may be shorter.
pub const CHUNK_SIZE: usize = 4_096;

/// A leaf of the chunk table: one XXH3-64, a little-endian u64.
pub const LEAF_SIZE: usize = 8;

/// The frame format's version, the header's `protocol` field.
pub const PROTOCOL_VERSION: u16 = 1;

/// The command numbers reserved for stage 10's replica commands. No command of this protocol takes one, so a frame
/// naming one is refused as an unknown command until those commands exist.
pub const REPLICA_COMMANDS: RangeInclusive<u16> = 256..=511;

/// The numbers of retired commands. A retired command's number is never given to another command, and a frame
/// carrying one is refused as an unknown command. No command has been retired.
pub const RETIRED_COMMANDS: &[u16] = &[];

/// The header flag of a stream's last frame.
pub const FLAG_LAST: u8 = 1;

/// An index naming nothing: an unbound cell of a result batch, an absent string, an absent term.
pub const ABSENT: u32 = u32::MAX;

/// The deepest a triple term of a request's term table nests: a term that is not a triple term has depth 0, a triple
/// term one more than the deepest of its three. It bounds the recursion of anything that walks a term, a conversion
/// to another term model above all.
pub const MAX_TERM_DEPTH: usize = 32;
