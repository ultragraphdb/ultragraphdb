/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What tells a posting apart in its list, in the list's order: its uid and, in a list that compares values, a value
/// posting's value. Where a list's iteration resumes ([`crate::List::iterate_after`]), since postings of one list may share
/// a uid: after the posting, or at it, whether or not the list holds it, when the position came from a reader and no walk
/// took it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PostingIdentity {
    uid: u128,
    value: Vec<u8>,
    included: bool,
}

impl PostingIdentity {
    /// The identity of a posting keyed `uid` holding `value`, empty for a reference.
    #[must_use]
    pub fn new(uid: u128, value: Vec<u8>) -> Self {
        Self {
            uid,
            value,
            included: false,
        }
    }

    /// The same identity, an iteration resuming at it reading the posting itself when the list holds it.
    #[must_use]
    pub fn included(mut self) -> Self {
        self.included = true;
        self
    }

    /// Whether an iteration resuming at this identity reads the posting itself.
    #[must_use]
    pub fn is_included(&self) -> bool {
        self.included
    }

    /// The posting's uid.
    #[must_use]
    pub fn uid(&self) -> u128 {
        self.uid
    }

    /// The posting's value, empty for a reference and in a list that does not compare values.
    #[must_use]
    pub fn value(&self) -> &[u8] {
        &self.value
    }

    /// Becomes the identity `(uid, value)` of a posting a walk took, which it resumes after, keeping its buffer.
    pub(crate) fn assign(&mut self, uid: u128, value: &[u8]) {
        self.included = false;
        self.uid = uid;
        self.value.clear();
        self.value.extend_from_slice(value);
    }
}
