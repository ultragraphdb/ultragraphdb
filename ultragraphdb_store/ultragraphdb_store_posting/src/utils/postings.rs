/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/list.go (createDeleteAllPosting, hasDeleteAll, getLengthDelta, isPlistEmpty).
 * Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_codec::{Posting, PostingList};

use crate::utils::constants::{DEL, SET};

/// The value a delete-all deletes, Dgraph's `_STAR_ALL`.
const STAR_ALL: &[u8] = b"_STAR_ALL";

/// The posting of a delete-all: a delete of the value `_STAR_ALL` at uid `u64::MAX`.
pub fn delete_all_posting() -> Posting {
    Posting::default()
        .with_op(DEL)
        .with_value(STAR_ALL.to_vec())
        .with_uid(u64::MAX)
}

/// Whether `posting` is a delete-all: a delete of `_STAR_ALL` without a language.
pub fn has_delete_all(posting: &Posting) -> bool {
    posting.op() == DEL && posting.value() == STAR_ALL && posting.lang_tag().is_empty()
}

/// What a posting's operation adds to a list's length.
pub fn length_delta(op: u32) -> i64 {
    match op {
        DEL => -1,
        SET => 1,
        _ => 0,
    }
}

/// Whether `plist` holds nothing: no splits and no uids.
pub fn is_plist_empty(plist: &PostingList) -> bool {
    plist.splits().is_empty() && plist.pack().is_none_or(|p| p.blocks().is_empty())
}
