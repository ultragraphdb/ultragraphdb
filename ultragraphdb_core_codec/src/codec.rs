/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, codec/codec.go. Changed for ultragraphdb: translated to Rust; DecodeToBuffer writes
 * to a Vec<u8> rather than a z.Buffer, and FreePack and CopyUidPack have no counterpart: a pack is freed when
 * dropped and copied by Clone.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_proto_pb::pb::UidPack;

use crate::types::decoder::Decoder;
use crate::types::encoder::Encoder;
use crate::types::seek_pos::SeekPos;

const BIT_MASK: u64 = 0xffff_ffff_0000_0000;

/// Packs `uids` into blocks of `block_size`, the last block holding fewer. Within each block the first UID is the
/// base and every next UID is stored as its delta from the previous one, group-varint encoded. `None` for an empty
/// list, as Go's `Encode` returns nil.
pub fn encode(uids: &[u64], block_size: usize) -> Option<UidPack> {
    let mut enc = Encoder::new(block_size);
    for &uid in uids {
        enc.add(uid);
    }
    enc.done()
}

/// Same as [`encode`], but reads the UIDs from `buf` as uvarint deltas, the format [`decode_to_buffer`] writes.
///
/// # Panics
///
/// When `buf` holds a truncated or overlong varint (Go's loop never ends on the first and panics on the second).
pub fn encode_from_buffer(buf: &[u8], block_size: usize) -> Option<UidPack> {
    let mut enc = Encoder::new(block_size);
    let mut prev = 0u64;
    let mut buf = buf;
    while !buf.is_empty() {
        let (uid, n) = uvarint(buf);
        buf = &buf[n..];

        let next = prev.wrapping_add(uid);
        enc.add(next);
        prev = next;
    }
    enc.done()
}

/// The number of UIDs the pack's blocks could hold, its block count times its block size.
pub fn approx_len(pack: &UidPack) -> usize {
    pack.blocks.len() * pack.block_size as usize
}

/// The number of UIDs in the pack.
pub fn exact_len(pack: &UidPack) -> usize {
    // num_uids includes the base UID.
    pack.blocks.iter().map(|b| b.num_uids as usize).sum()
}

/// The pack's UIDs at or after `seek`, in order.
pub fn decode(pack: &UidPack, seek: u64) -> Vec<u64> {
    let mut out = Vec::with_capacity(approx_len(pack));
    let mut dec = Decoder::unpositioned(pack);

    let mut uids = dec.seek(seek, SeekPos::Start);
    while !uids.is_empty() {
        out.extend_from_slice(uids);
        uids = dec.next_block();
    }
    out
}

/// Appends the pack's UIDs to `buf` as uvarint deltas, each from the previous UID, the first from 0.
pub fn decode_to_buffer(buf: &mut Vec<u8>, pack: &UidPack) {
    let mut last = 0u64;
    let mut dec = Decoder::unpositioned(pack);
    let mut uids = dec.seek(0, SeekPos::Start);
    while !uids.is_empty() {
        for &u in uids {
            put_uvarint(buf, u.wrapping_sub(last));
            last = u;
        }
        uids = dec.next_block();
    }
}

/// Whether two UIDs share their upper 32 bits, and so may sit in one block.
pub(crate) fn match32_msb(num1: u64, num2: u64) -> bool {
    (num1 & BIT_MASK) == (num2 & BIT_MASK)
}

/// Go's `binary.PutUvarint`.
fn put_uvarint(buf: &mut Vec<u8>, mut x: u64) {
    while x >= 0x80 {
        buf.push(x as u8 | 0x80);
        x >>= 7;
    }
    buf.push(x as u8);
}

/// Go's `binary.Uvarint`: the value and the number of bytes it took.
fn uvarint(buf: &[u8]) -> (u64, usize) {
    let mut x = 0u64;
    for (i, &b) in buf.iter().enumerate().take(10) {
        if b < 0x80 {
            assert!(i < 9 || b <= 1, "uvarint overflows a u64");
            return (x | u64::from(b) << (7 * i), i + 1);
        }
        x |= u64::from(b & 0x7f) << (7 * i);
    }
    panic!("truncated or overlong uvarint");
}
