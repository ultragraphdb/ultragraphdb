/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Round frames as the wire's own builder writes them: the independent measure the step's size checks are tested
//! against, apart from the step's own arithmetic.

use ultragraphdb_protocol_wire::{BodyBuilder, Command, Header, RoundBody};

/// The frame of a Round committing `records`, in order, `None` past the largest frame.
pub fn round_frame(records: &[Vec<u8>]) -> Option<usize> {
    let mut body = RoundBody::new(0, 0, 0, 2);
    for record in records {
        body.commit(record.clone());
    }
    body.frame(Header::new(0, Command::Round))
        .ok()
        .map(|f| f.len())
}

/// The largest record a Round of it alone holds within `bytes` of frame, 0 when none does: a binary search over record
/// lengths, each measured by the builder.
pub fn largest_record(bytes: usize) -> usize {
    let fits = |len: usize| round_frame(&[vec![0; len]]).is_some_and(|f| f <= bytes);
    if !fits(0) {
        return 0;
    }
    let (mut low, mut high) = (0usize, bytes);
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if fits(mid) {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    low
}
