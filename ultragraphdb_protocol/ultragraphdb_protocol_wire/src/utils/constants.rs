/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::ops::RangeInclusive;

// A `usize` is 32 to 64 bits wide on every target the crate builds for, so a `u32` field widens into a `usize` and a
// `usize` length into a `u64` field without loss, which every such `as` cast in the crate relies on.
const _: () = assert!(usize::BITS >= 32);
const _: () = assert!(usize::BITS <= 64);

/// The header's length, in front of every frame: client frames, log records and replica frames.
pub const HEADER_SIZE: usize = 256;

/// The command header's length, the header's last 128 bytes, laid out by each command.
pub const COMMAND_HEADER_SIZE: usize = 128;

/// The largest frame, header and body: 1 MiB, a multiple of 4,096 so a log record can be written with direct I/O.
pub const MAX_FRAME_SIZE: usize = 1 << 20;

/// The least `max_frame` a Hello or a Welcome announces: a Result frame of one term of the largest encoding, a vector
/// of 65,535 dimensions (262,143 bytes), fits with room to spare.
pub const MAX_FRAME_FLOOR: usize = 1 << 19;

/// The largest write-set record a transaction commits by default, the Welcome's `max_write_set`: what one Round frame
/// holds beside its header, its command header's sections and one decision.
pub const DEFAULT_MAX_WRITE_SET: u32 = 1_040_384;

/// The longest a read-write transaction lives by default, the Welcome's `transaction_ms`.
pub const DEFAULT_TRANSACTION_MS: u32 = 10_000;

/// The streams a connection holds in flight by default, the Welcome's `max_streams`.
pub const DEFAULT_MAX_STREAMS: u32 = 256;

/// The Result frames a read's stream sends before a Credit by default, the Welcome's `initial_credit`.
pub const DEFAULT_INITIAL_CREDIT: u32 = 4;

/// The body's first chunk: the rest of the frame's first 4,096-byte block after the header.
pub const FIRST_CHUNK_SIZE: usize = 3_840;

/// Every later chunk, one 4,096-byte block of the frame; the last may be shorter.
pub const CHUNK_SIZE: usize = 4_096;

/// A leaf of the chunk table: one XXH3-64, a little-endian u64.
pub const LEAF_SIZE: usize = 8;

/// The frame format's version, the header's `protocol` field.
pub const PROTOCOL_VERSION: u16 = 1;

/// The command numbers reserved for stage 8's replica commands. No command of this protocol takes one, so a frame
/// naming one is refused as an unknown command until those commands exist.
pub const REPLICA_COMMANDS: RangeInclusive<u16> = 256..=511;

/// The command numbers of the role commands (`PROTOCOL.md` §3, §13): roles exchange them on role connections, and a
/// client endpoint refuses every one as a command it does not serve. 512 to 531 are assigned; the rest are reserved
/// and refused as unknown commands.
pub const ROLE_COMMANDS: RangeInclusive<u16> = 512..=767;

/// The largest epoch number, 2⁴² − 1: an epoch field holds 1 to this, or 0 where the field allows none.
pub const MAX_EPOCH: u64 = (1 << 42) - 1;

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

/// The fixed part of a write-set record (`PROTOCOL.md` §12.2): kind, tenant, catalog, read version, session,
/// request and principal.
pub const RECORD_FIXED_SIZE: usize = 64;

/// The sections of a write-set record.
pub const RECORD_SECTIONS: usize = 12;

/// The size of a write-set record's operation, section 3: `operation` u16, 6 zero bytes, a 16-byte `argument`.
pub const RECORD_OPERATION_SIZE: usize = 24;

/// The size of a write-set record's registration, section 4: `entity` system ID, `group` u32, 4 zero bytes,
/// `generation` u64.
pub const RECORD_REGISTRATION_SIZE: usize = 32;

/// The size of a commit-time operation's result (`PROTOCOL.md` §6.3): `value` i64, `node` u32, `created` u8, 3 zero
/// bytes; a Commit reply's section 0 and a write-set record's section 9.
pub const OPERATION_RESULT_SIZE: usize = 16;

/// The size of a Scan's range record (`PROTOCOL.md` §13.13): `partition` u32, `after_len` u32, `first`, `first_len`,
/// `past` and `past_len`, u32s.
pub const SCAN_RANGE_SIZE: usize = 24;

/// The streams a role endpoint's Welcome announces (`PROTOCOL.md` §11.12): what a role connection carries is already
/// bounded by the frontend's bounds on its clients' connections and streams.
pub const ROLE_MAX_STREAMS: u32 = u32::MAX;

/// The most result columns a quad pattern's batch holds: its four slots unbound and the reifier column.
pub const PATTERN_COLUMNS_MAX: u32 = 5;

/// The most result columns a query's batch holds, its projections.
pub const QUERY_COLUMNS_MAX: u32 = 1_024;

/// What a Query's frame takes beside its text: a query text is at most the receiver's `max_frame` less this.
pub const QUERY_TEXT_RESERVE: u32 = 512;

/// The longest name an Install's source takes, in bytes.
pub const SOURCE_NAME_MAX: usize = 255;

/// The `detail` of an Error 1 answering a Scan that names a transaction the partition does not hold (`PROTOCOL.md`
/// §9.4), which compute relays and the frontend holds until the Bind stream ends.
pub const DETAIL_TRANSACTION_NOT_HELD: u64 = 1;

/// Bit 0 of the `detail` of an Error 8 (OperationFailed): the value found, in its sections 1 and 2, is cut to fit the frame
/// within the floor of `max_frame`, or left out when no cut keeps it whole (`PROTOCOL.md` §9.4); the client reads the
/// value again for it whole.
pub const DETAIL_VALUE_CUT: u64 = 1;

/// A version's sequence bits: a version holds them even, an epoch's version bound all ones.
pub(crate) const VERSION_SEQUENCE_MASK: u64 = (1 << 22) - 1;

/// The keyspace byte a dictionary key begins with, which tells a one-key dictionary range (`PROTOCOL.md` §13.13).
pub(crate) const DICTIONARY_KEYSPACE: u8 = 0x1B;
