/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, codec/codec.go. Changed for ultragraphdb: translated to Rust; lists of 128-bit system
 * IDs in place of 64-bit UIDs, refused when not strictly increasing; DecodeToBuffer writes LEB128 deltas of the IDs'
 * 128-bit values to a Vec<u8> rather than a z.Buffer, and FreePack and CopyUidPack have no counterpart: a pack is
 * freed when dropped and copied by Clone.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_ids::SystemId;

use crate::errors::codec_error::CodecError;
use crate::types::decoder::Decoder;
use crate::types::encoder::Encoder;
use crate::types::seek_pos::SeekPos;
use crate::types::uid_pack::UidPack;
use crate::utils::leb128;

/// Packs `ids` into blocks of `block_size`, the last block holding fewer. Within each block the first ID is stored
/// whole and every next ID as the LEB128 difference of its first 104 bits from the previous one's. `None` for an empty
/// list, as Go's `Encode` returns nil.
///
/// # Errors
///
/// [`CodecError::out_of_order`] naming the first position whose ID is not greater than the one before it.
pub fn encode(ids: &[SystemId], block_size: usize) -> Result<Option<UidPack>, CodecError> {
    let mut enc = Encoder::new(block_size);
    for &id in ids {
        enc.add(id)?;
    }
    let pack = enc.done();
    debug_assert_eq!(
        pack.as_ref().map_or(0, exact_len),
        ids.len(),
        "every ID is packed"
    );
    Ok(pack)
}

/// Same as [`encode`], but reads the IDs from `buf` as LEB128 deltas of their 128-bit values, the first from 0, the
/// format [`decode_to_buffer`] writes.
///
/// # Errors
///
/// [`CodecError::truncated_varint`] when `buf` ends inside a varint, [`CodecError::overflowing_varint`] for a
/// varint or a running sum that does not fit 128 bits, [`CodecError::non_canonical_varint`] for a varint with a
/// redundant last group, [`CodecError::invalid_id`] for a value that is not a valid system ID, and
/// [`CodecError::out_of_order`] for a delta of 0.
pub fn encode_from_buffer(buf: &[u8], block_size: usize) -> Result<Option<UidPack>, CodecError> {
    let mut enc = Encoder::new(block_size);
    let (mut previous, mut offset, mut position) = (0u128, 0usize, 0usize);
    // Each pass reads at least one byte of buf, so the loop ends at its end.
    while let Some(rest) = buf.get(offset..).filter(|rest| !rest.is_empty()) {
        let (delta, len) = leb128::read(rest, offset, leb128::U128_LEN_MAX)?;
        let value = previous
            .checked_add(delta)
            .ok_or_else(|| CodecError::overflowing_varint(offset))?;
        let id = SystemId::from_u128(value).map_err(|e| CodecError::invalid_id(position, e))?;
        enc.add(id)?;
        let next_offset = offset
            .checked_add(len)
            .ok_or_else(|| CodecError::truncated_varint(offset))?;
        let next_position = position
            .checked_add(1)
            .ok_or_else(|| CodecError::truncated_varint(offset))?;
        (previous, offset, position) = (value, next_offset, next_position);
    }
    debug_assert_eq!(offset, buf.len(), "every byte is read");
    Ok(enc.done())
}

/// The number of IDs the pack's blocks could hold, its block count times its block size.
#[must_use]
pub fn approx_len(pack: &UidPack) -> usize {
    let block_size = usize::try_from(pack.block_size()).unwrap_or(usize::MAX);
    pack.blocks().len().saturating_mul(block_size)
}

/// The number of IDs in the pack.
#[must_use]
pub fn exact_len(pack: &UidPack) -> usize {
    pack.blocks()
        .iter()
        .map(|b| usize::try_from(b.count()).unwrap_or(usize::MAX))
        .fold(0, usize::saturating_add)
}

/// The pack's IDs at or after `from`, any 128-bit value, in order.
#[must_use]
pub fn decode(pack: &UidPack, from: u128) -> Vec<SystemId> {
    let mut out = Vec::with_capacity(exact_len(pack));
    let mut dec = Decoder::sought(pack, from, SeekPos::Start);
    // Each pass moves to the next block holding IDs, so the loop ends after the last block.
    while !dec.ids().is_empty() {
        out.extend_from_slice(dec.ids());
        let _next: &[SystemId] = dec.next_block();
    }
    debug_assert!(
        out.windows(2).all(|w| matches!(w, [a, b] if a < b)),
        "a pack decodes in order"
    );
    out
}

/// Appends the pack's IDs to `buf` as LEB128 deltas of their 128-bit values, each from the previous ID, the first
/// from 0.
pub fn decode_to_buffer(buf: &mut Vec<u8>, pack: &UidPack) {
    let mut last = 0u128;
    let mut dec = Decoder::new(pack);
    // Each pass moves to the next block holding IDs, so the loop ends after the last block.
    while !dec.ids().is_empty() {
        for id in dec.ids() {
            let value = id.to_u128();
            debug_assert!(value > last, "a pack's IDs strictly increase");
            leb128::put(buf, value.saturating_sub(last));
            last = value;
        }
        let _next: &[SystemId] = dec.next_block();
    }
}
