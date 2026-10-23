/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/list.go (MutableLayer.insertPosting). Changed for ultragraphdb: translated to
 * Rust; Dgraph asserts that the posting's start timestamp is the layer's read timestamp, which the port's list
 * guarantees by giving every posting its own read timestamp.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_proto_pb::pb::{Posting, PostingList};

use crate::types::mutable_layer::{MutableLayer, uid_index};
use crate::utils::constants::DEL;

impl MutableLayer {
    /// Adds `mpost` to the scratch layer, replacing the posting of its uid there: Dgraph's `insertPosting`. With a
    /// count index, a delete of a uid set in this scratch layer removes the set instead of recording the delete, so
    /// that the layer's length stays a count of its sets.
    pub(crate) fn insert_posting(&mut self, mpost: Posting, has_count_index: bool) {
        self.read_ts = mpost.start_ts;
        let current = self.current.get_or_insert_with(PostingList::default);
        if mpost.uid == 0 {
            current.postings.push(mpost);
            return;
        }
        let at = self.current_uids.get(&mpost.uid).copied();
        match at.and_then(|at| current.postings.get_mut(at).map(|slot| (at, slot))) {
            Some((at, slot)) if has_count_index && mpost.op == DEL && slot.op != DEL => {
                current.postings.remove(at);
                self.current_uids = uid_index(current);
            }
            Some((_, slot)) => *slot = mpost,
            None => {
                self.current_uids.insert(mpost.uid, current.postings.len());
                current.postings.push(mpost);
            }
        }
    }
}
