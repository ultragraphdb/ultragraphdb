/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeSet, VecDeque};

use ultragraphdb_protocol_wire::{
    BodyBuilder, Check, CheckKind, Command, Ended, Header, MAX_FRAME_SIZE, SessionCheck,
};

/// The bytes one check takes in a Check frame's section 0 (`PROTOCOL.md` §13.24).
const CHECK_BYTES: usize = 32;
/// The bytes one session takes in an Ended frame's section 0 (`PROTOCOL.md` §13.25).
const SESSION_BYTES: usize = 16;

/// The Endeds and Checks an endpoint core's requests queued for the system group, and the frames of one turn
/// (`PROTOCOL.md` §11.5, *Turns*): a turn takes one Ended frame of distinct sessions and then one Check frame of one check
/// per session, each ascending by session and holding as many as a frame of `max_frame` takes; a session's further check,
/// and what a frame cannot hold, waits for the next turn, in the order it was queued. Sans-I/O: the link's sending task
/// runs a turn each time it runs, and a test runs one exactly.
///
/// Data plane (Ultra rule 3): its queues hold what the core's requests queued, which their connections' bounds bound.
#[derive(Debug, Default, Clone)]
pub struct SystemTurns {
    checks: VecDeque<(u64, SessionCheck)>,
    ended: BTreeSet<[u8; 16]>,
    next_ticket: u64,
}

/// What one turn sends: the sessions of its Ended frame and the checks of its Check frame, each ascending by session, each
/// check with the ticket its queueing gave.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Turn {
    ended: Vec<[u8; 16]>,
    checks: Vec<(u64, SessionCheck)>,
}

impl SystemTurns {
    /// Nothing queued.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues the check of `session` for `principal`, of `kind`: the ticket its outcome is told by.
    pub fn queue_check(&mut self, session: [u8; 16], principal: u64, kind: CheckKind) -> u64 {
        let ticket = self.next_ticket;
        self.next_ticket = ticket.wrapping_add(1);
        self.checks
            .push_back((ticket, SessionCheck::new(session, principal, kind)));
        ticket
    }

    /// Queues the Ended of `session`; a session queued already is queued once.
    pub fn queue_ended(&mut self, session: [u8; 16]) {
        let _new = self.ended.insert(session);
    }

    /// Takes back the check of `ticket` that has not left: whether it was queued.
    pub fn withdraw(&mut self, ticket: u64) -> bool {
        let before = self.checks.len();
        self.checks.retain(|(queued, _)| *queued != ticket);
        self.checks.len() != before
    }

    /// Whether nothing is queued.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.checks.is_empty() && self.ended.is_empty()
    }

    /// One turn: at most `ended_max` sessions of the queued Endeds and at most `checks_max` checks, one per session, each the
    /// session's first queued, both the lowest sessions; the rest stay queued in their order.
    pub fn take(&mut self, checks_max: usize, ended_max: usize) -> Turn {
        let ended: Vec<[u8; 16]> = self.ended.iter().take(ended_max).copied().collect();
        for session in &ended {
            let _gone = self.ended.remove(session);
        }
        // Each session's first check, in its queue's order; then the lowest sessions among them.
        let mut firsts: Vec<(u64, SessionCheck)> = Vec::new();
        let mut seen = BTreeSet::new();
        for (ticket, check) in &self.checks {
            if seen.insert(check.session()) {
                firsts.push((*ticket, *check));
            }
        }
        firsts.sort_by_key(|(_, check)| check.session());
        firsts.truncate(checks_max);
        let taken: BTreeSet<u64> = firsts.iter().map(|(ticket, _)| *ticket).collect();
        self.checks.retain(|(ticket, _)| !taken.contains(ticket));
        Turn {
            ended,
            checks: firsts,
        }
    }

    /// The checks one Check frame of at most `max_frame` bytes holds, at least one.
    #[must_use]
    pub fn checks_max(max_frame: u32) -> usize {
        let build = |count: usize| {
            let checks = (0..count)
                .map(|at| SessionCheck::new(session_at(at), 0, CheckKind::Asks))
                .collect();
            Check::new(checks)
                .frame(Header::new(0, Command::Check))
                .ok()
                .map(|f| f.len())
        };
        per_frame(build, CHECK_BYTES, max_frame)
    }

    /// The sessions one Ended frame of at most `max_frame` bytes holds, at least one.
    #[must_use]
    pub fn ended_max(max_frame: u32) -> usize {
        let build = |count: usize| {
            let sessions = (0..count).map(session_at).collect();
            Ended::new(sessions)
                .frame(Header::new(0, Command::Ended))
                .ok()
                .map(|f| f.len())
        };
        per_frame(build, SESSION_BYTES, max_frame)
    }
}

impl Turn {
    /// The sessions of its Ended frame, ascending.
    #[must_use]
    pub fn ended(&self) -> &[[u8; 16]] {
        &self.ended
    }

    /// The checks of its Check frame with their tickets, ascending by session.
    #[must_use]
    pub fn checks(&self) -> &[(u64, SessionCheck)] {
        &self.checks
    }

    /// Whether it sends nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ended.is_empty() && self.checks.is_empty()
    }

    /// Its Ended frame on `header`'s stream, `None` when it ends no session.
    #[must_use]
    pub fn ended_frame(&self, header: Header) -> Option<Vec<u8>> {
        if self.ended.is_empty() {
            return None;
        }
        Ended::new(self.ended.clone()).frame(header).ok()
    }

    /// Its Check frame on `header`'s stream, `None` when it checks no session.
    #[must_use]
    pub fn check_frame(&self, header: Header) -> Option<Vec<u8>> {
        if self.checks.is_empty() {
            return None;
        }
        let checks = self.checks.iter().map(|(_, check)| *check).collect();
        Check::new(checks).frame(header).ok()
    }
}

/// The entries of `unit` bytes a frame takes within `max_frame`, at least one: the largest count, up to the count the bytes
/// past a frame of one entry hold, whose frame `build` makes within the bound; the chunk table grows with the frame's length.
/// Found from a few frames built, near the count predicted, once per connection, by the link, not per turn.
fn per_frame(build: impl Fn(usize) -> Option<usize>, unit: usize, max_frame: u32) -> usize {
    let bound =
        usize::try_from(max_frame).map_or(MAX_FRAME_SIZE, |bound| bound.min(MAX_FRAME_SIZE));
    let Some(one) = build(1) else {
        return 1;
    };
    // A frame of more entries is longer, so the counts that fit are those up to the last that does.
    let fits = |count: usize| build(count).is_some_and(|len| len <= bound);
    let most = bound
        .saturating_sub(one)
        .checked_div(unit)
        .map_or(1, |more| more.saturating_add(1));
    // A frame of half as many entries measures what an entry takes, its share of the chunk table included, which grows in
    // proportion to the frame's length: the count those bytes predict for the bound is the last that fits or a step from it.
    let half = (most / 2).max(2);
    let predicted = build(half)
        .and_then(|len| {
            bound
                .saturating_sub(one)
                .saturating_mul(half - 1)
                .checked_div(len.saturating_sub(one))
        })
        .map_or(most, |more| more.saturating_add(1));
    let mut count = predicted.clamp(1, most);
    if fits(count) {
        // Each step adds one entry, up to `most` (Ultra rule 2).
        while count < most && fits(count + 1) {
            count += 1;
        }
    } else {
        // Each step drops one entry (Ultra rule 2).
        while count > 1 && !fits(count) {
            count -= 1;
        }
    }
    count
}

/// A session numbered `at` from 1, ascending: the sessions of a frame built to measure one.
fn session_at(at: usize) -> [u8; 16] {
    u128::try_from(at)
        .map_or(u128::MAX, |at| at.saturating_add(1))
        .to_be_bytes()
}
