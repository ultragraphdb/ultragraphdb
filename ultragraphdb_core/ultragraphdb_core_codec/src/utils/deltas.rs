/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A block's deltas: each the difference of an ID's first 104 bits (type, sequence and bucket) from the previous
//! ID's, in LEB128. Data plane: the IDs are handed to the caller one by one, with no buffer of their own.

use ultragraphdb_core_ids::SystemId;

use crate::errors::codec_error::CodecError;
use crate::utils::leb128;

/// The reserved bits below an ID's first 104.
const RESERVED_BITS: u32 = 24;
/// The most bytes a delta takes: 104 bits need 15 groups of seven.
const DELTA_LEN_MAX: usize = 15;
/// The first value past 104 bits.
const TOP_104_END: u128 = 1 << 104;

const _: () = assert!(DELTA_LEN_MAX * 7 >= 104 && (DELTA_LEN_MAX - 1) * 7 < 104);

/// An ID's first 104 bits.
pub(crate) fn top_104(id: SystemId) -> u128 {
    id.to_u128() >> RESERVED_BITS
}

/// Appends the delta from `previous` to `next`, which is above it.
pub(crate) fn put(w: &mut Vec<u8>, previous: SystemId, next: SystemId) {
    debug_assert!(previous < next, "a block's IDs strictly increase");
    leb128::put(w, top_104(next).saturating_sub(top_104(previous)));
}

/// Walks the `count` IDs of the block whose first ID is `first` and whose deltas are `deltas`, handing each to
/// `each` in order, and returns the last, or `first` for an empty block. Each ID is handed over only once its delta
/// is read and checked, so `each` sees a prefix of the IDs when the walk fails.
///
/// # Errors
///
/// The refusals of [`UidBlock::new`](crate::UidBlock::new), each naming the position of the ID or the byte of the
/// deltas at fault.
pub(crate) fn walk(
    first: SystemId,
    deltas: &[u8],
    count: u32,
    mut each: impl FnMut(SystemId),
) -> Result<SystemId, CodecError> {
    if count == 0 {
        return if deltas.is_empty() {
            Ok(first)
        } else {
            Err(CodecError::trailing_deltas(0))
        };
    }
    each(first);
    let (mut previous, mut last, mut offset) = (top_104(first), first, 0usize);
    // At most count - 1 passes, and each reads at least one byte of the deltas or fails.
    for position in 1..usize::try_from(count).unwrap_or(usize::MAX) {
        let rest = deltas.get(offset..).unwrap_or_default();
        let (delta, len) = leb128::read(rest, offset, DELTA_LEN_MAX)?;
        if delta == 0 {
            return Err(CodecError::out_of_order(position));
        }
        let next = previous
            .checked_add(delta)
            .filter(|next| *next < TOP_104_END)
            .ok_or_else(|| CodecError::delta_overflow(position))?;
        last = SystemId::from_u128(next << RESERVED_BITS)
            .map_err(|e| CodecError::invalid_id(position, e))?;
        each(last);
        previous = next;
        offset += len;
    }
    if offset != deltas.len() {
        return Err(CodecError::trailing_deltas(offset));
    }
    debug_assert!(last >= first, "the last ID is at or above the first");
    Ok(last)
}
