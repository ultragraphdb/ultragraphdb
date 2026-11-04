/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod referrer_set_codec;

use std::num::NonZeroU64;

use ultragraphdb_core_ids::{BucketGroup, Epoch};

use crate::types::referrer_member::ReferrerMember;

/// An entity's referrer set `R(o)` as its home holds it, the value of its key in the referrers keyspace (partitions,
/// "An entity's referrer set names where its inbound edges may be"): a superset of the subject bucket groups holding
/// edges to the entity, at most one member per group, sorted by group. It only grows in this stage: a member is
/// removed by the reclamation rule, which stage 8 builds.
///
/// The value, version byte 1: the byte 1, the member count as a u32 big-endian, then each member, sorted by group, as
/// its group u32, its epoch u64 and its generation u64, all big-endian.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReferrerSet {
    members: Vec<ReferrerMember>,
}

/// What a registration did to a referrer set: the member's generation it answers, and whether the set changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Registered {
    /// The member's generation after the registration.
    pub generation: NonZeroU64,
    /// Whether the registration added or renewed the member.
    pub changed: bool,
}

impl ReferrerSet {
    /// The empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// The members, sorted by group.
    pub fn members(&self) -> &[ReferrerMember] {
        &self.members
    }

    /// The member for `group`, if any.
    pub fn member(&self, group: BucketGroup) -> Option<&ReferrerMember> {
        self.position(group)
            .ok()
            .and_then(|at| self.members.get(at))
    }

    /// The members a read at `epoch` sees: those added at or below it.
    pub fn at(&self, epoch: Epoch) -> impl Iterator<Item = &ReferrerMember> {
        self.members.iter().filter(move |m| m.epoch() <= epoch)
    }

    /// Applies a registration of `group` naming `named`, the generation the registrant last saw (0 for none), at the
    /// home's open epoch `epoch` (partitions, "An entity's referrer set names where its inbound edges may be"): a
    /// registration naming the member's current generation, or any generation for a group not in the set, adds or
    /// renews the member at one generation above the larger of the generation named and the highest the group ever
    /// held, keeping a renewed member's epoch; one naming a generation below the current one changes nothing and
    /// answers the current generation. With no removal in this stage, the highest a group ever held is its current
    /// generation.
    pub fn register(&mut self, group: BucketGroup, named: u64, epoch: Epoch) -> Registered {
        let at = self.position(group);
        let current = at
            .ok()
            .and_then(|i| self.members.get(i))
            .map(ReferrerMember::generation);
        if let Some(current) = current
            && named < current.get()
        {
            return Registered {
                generation: current,
                changed: false,
            };
        }
        // No member is removed in this stage, so the highest generation the group ever held is its current one, and
        // the larger of it and `named` is `named`.
        let generation = NonZeroU64::MIN.saturating_add(named);
        self.put(at, group, generation, epoch);
        Registered {
            generation,
            changed: true,
        }
    }

    /// Sets `group`'s member to `generation`, as a home's logged registration names it: its epoch kept when the group
    /// is a member, `epoch` otherwise. What replay and the apply of a registration's round write.
    pub fn set(&mut self, group: BucketGroup, generation: NonZeroU64, epoch: Epoch) {
        let at = self.position(group);
        self.put(at, group, generation, epoch);
    }

    /// Where `group`'s member is, or where it goes.
    fn position(&self, group: BucketGroup) -> Result<usize, usize> {
        self.members
            .binary_search_by_key(&group, ReferrerMember::group)
    }

    /// Gives the member at `at`, found or to insert there, `generation`: a found member keeps its epoch, an inserted
    /// one takes `epoch`.
    fn put(
        &mut self,
        at: Result<usize, usize>,
        group: BucketGroup,
        generation: NonZeroU64,
        epoch: Epoch,
    ) {
        match at {
            Ok(i) => {
                if let Some(member) = self.members.get_mut(i) {
                    *member = ReferrerMember::new(group, member.epoch(), generation);
                }
            }
            Err(i) => self
                .members
                .insert(i, ReferrerMember::new(group, epoch, generation)),
        }
        debug_assert!(
            self.members
                .windows(2)
                .all(|w| matches!(w, [a, b] if a.group() < b.group())),
            "members stay sorted, one per group"
        );
    }
}
