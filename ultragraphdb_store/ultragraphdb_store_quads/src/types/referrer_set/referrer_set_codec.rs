/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU64;

use ultragraphdb_core_ids::{BucketGroup, Epoch};

use crate::errors::quad_error::QuadError;
use crate::types::referrer_member::ReferrerMember;
use crate::types::referrer_set::ReferrerSet;

/// The value's version byte.
const VERSION: u8 = 1;
/// The bytes before the members: the version and the count.
const HEADER: usize = 5;
/// The bytes of one member: its group, epoch and generation.
const MEMBER: usize = 4 + 8 + 8;

impl ReferrerSet {
    /// The set's value, appended to `out`.
    pub fn encode(&self, out: &mut Vec<u8>) {
        out.reserve(HEADER + MEMBER * self.members.len());
        out.push(VERSION);
        // At most one member per group, so the count is below 2^18 and fits.
        let count = u32::try_from(self.members.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&count.to_be_bytes());
        for member in &self.members {
            out.extend_from_slice(&member.group().get().to_be_bytes());
            out.extend_from_slice(&member.epoch().get().to_be_bytes());
            out.extend_from_slice(&member.generation().get().to_be_bytes());
        }
    }

    /// The set `value` holds, the value of `key`.
    ///
    /// # Errors
    ///
    /// [`QuadError::corrupted`] naming `key` for a version byte other than 1, a count the value does not hold, a value
    /// longer than its members, members not sorted by group or repeating one, a group past [`BucketGroup::COUNT`], a
    /// generation of 0, or an epoch out of range.
    ///
    /// [`BucketGroup::COUNT`]: ultragraphdb_core_ids::BucketGroup::COUNT
    pub fn decode(key: &[u8], value: &[u8]) -> Result<Self, QuadError> {
        let corrupted =
            |reason: &str| QuadError::corrupted(key, format!("a referrer set {reason}"));
        let (header, body) = value
            .split_first_chunk::<HEADER>()
            .ok_or_else(|| corrupted("is shorter than its header"))?;
        let [version, count @ ..] = *header;
        if version != VERSION {
            return Err(corrupted(&format!("has version {version}, not {VERSION}")));
        }
        let count = usize::try_from(u32::from_be_bytes(count)).unwrap_or(usize::MAX);
        // The length is checked before any member is allocated, so a corrupt count costs nothing.
        if count.checked_mul(MEMBER) != Some(body.len()) {
            return Err(corrupted(&format!(
                "of {count} members does not hold {} bytes of them",
                body.len()
            )));
        }
        let mut members: Vec<ReferrerMember> = Vec::with_capacity(count);
        let (chunks, _) = body.as_chunks::<MEMBER>();
        for chunk in chunks {
            let member = member(chunk).map_err(corrupted)?;
            if members
                .last()
                .is_some_and(|last| last.group() >= member.group())
            {
                return Err(corrupted("is not sorted by group, one member per group"));
            }
            members.push(member);
        }
        Ok(Self { members })
    }
}

/// The member of `chunk`, its 20 bytes, or why it is none.
fn member(chunk: &[u8]) -> Result<ReferrerMember, &'static str> {
    let (group, rest) = chunk
        .split_first_chunk::<4>()
        .ok_or("member is cut short")?;
    let (epoch, rest) = rest.split_first_chunk::<8>().ok_or("member is cut short")?;
    let generation: [u8; 8] = rest.try_into().map_err(|_| "member is cut short")?;
    let group =
        BucketGroup::new(u32::from_be_bytes(*group)).map_err(|_| "names a group past 2^18")?;
    let epoch = Epoch::new(u64::from_be_bytes(*epoch)).map_err(|_| "names an epoch past 2^42")?;
    let generation = NonZeroU64::new(u64::from_be_bytes(generation)).ok_or("names generation 0")?;
    Ok(ReferrerMember::new(group, epoch, generation))
}
