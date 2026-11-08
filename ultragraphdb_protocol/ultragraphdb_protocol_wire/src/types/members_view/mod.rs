/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod members_view_body_view;

use crate::types::member::Member;
use crate::types::string_list::StringList;
use crate::utils::whole_records::{record_at, record_count};

/// The body of a received Members frame, read in place; see [`crate::Members`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MembersView<'a> {
    members: &'a [u8],
    addresses: StringList<'a>,
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<MembersView<'static>>() <= 64);

impl<'a> MembersView<'a> {
    /// The number of members.
    #[must_use]
    pub fn len(&self) -> usize {
        record_count::<{ Member::SIZE }>(self.members)
    }

    /// Whether there is no member: a request, or an empty reply.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// Member `index`; `None` past the last.
    #[must_use]
    pub fn member(&self, index: usize) -> Option<Member> {
        let bytes = record_at::<{ Member::SIZE }>(self.members, index)?;
        Member::decode(0, 0, bytes).ok()
    }

    /// Member `index`'s role address; `None` past the last.
    #[must_use]
    pub fn address(&self, index: usize) -> Option<&'a str> {
        self.addresses.get(self.member(index)?.address() as usize) // lossless: a `usize` is at least 32 bits
    }

    /// The body's string list of addresses.
    #[must_use]
    pub fn addresses(&self) -> StringList<'a> {
        self.addresses
    }
}
