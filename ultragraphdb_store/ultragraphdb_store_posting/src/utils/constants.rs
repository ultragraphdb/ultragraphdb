/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/list.go, posting/lists.go and posting/mvcc.go (the constants). Changed for
 * ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// A posting's operation: set, which adds one to a list's length.
pub const SET: u32 = 0x01;
/// A posting's operation: delete, which takes one from a list's length.
pub const DEL: u32 = 0x02;
/// A posting's operation: overwrite, which leaves a list's length.
pub const OVR: u32 = 0x03;

/// The meta byte of a schema or type entry.
pub const BIT_SCHEMA_POSTING: u8 = 0x01;
/// The meta byte of a delta: the postings one commit wrote.
pub const BIT_DELTA_POSTING: u8 = 0x04;
/// The meta byte of a complete posting list.
pub const BIT_COMPLETE_POSTING: u8 = 0x08;
/// The meta byte of an empty posting list.
pub const BIT_EMPTY_POSTING: u8 = 0x10;

/// The encoded size at which a rollup splits a list spanning more than one block: Dgraph's `maxListSize`, half a
/// MiB.
pub const MAX_LIST_SIZE: usize = (1 << 20) / 2;

/// The block size of every posting list's pack.
pub const BLOCK_SIZE: usize = 256;

/// The number of deltas above which a list is rolled up first: `checkForRollup` and `ReadPostingList` send such a
/// key to the high-priority queue.
pub const HIGH_PRIORITY_DELTAS: usize = 500;
