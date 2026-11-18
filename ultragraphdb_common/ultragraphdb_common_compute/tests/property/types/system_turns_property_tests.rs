/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A turn's frames: laws L5.5 and L5.6 of `notes/laws.md`, a state machine against a model of `PROTOCOL.md` §11.5, *Turns*.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_common_compute::SystemTurns;
use ultragraphdb_protocol_wire::{CheckKind, SessionCheck};

const SESSIONS: [u128; 4] = [3, 1, 4, 2];
const ALICE: u64 = 11;

/// The model: each session's checks in their queue order, each with its ticket; the Endeds as a set.
struct Turns {
    turns: SystemTurns,
    queued: BTreeMap<u128, VecDeque<(u64, CheckKind)>>,
    ended: BTreeSet<u128>,
    sent: Vec<u64>,
    tickets: Vec<u64>,
}

#[hegel::state_machine]
impl Turns {
    #[rule]
    fn check(&mut self, tc: TestCase) {
        let session = tc.draw(gs::sampled_from(SESSIONS.to_vec()));
        let kind = tc.draw(gs::sampled_from(vec![CheckKind::Asks, CheckKind::Marks]));
        let ticket = self.turns.queue_check(session.to_be_bytes(), ALICE, kind);
        assert!(
            !self.tickets.contains(&ticket),
            "a ticket is never given twice"
        );
        self.tickets.push(ticket);
        self.queued
            .entry(session)
            .or_default()
            .push_back((ticket, kind));
    }

    #[rule]
    fn ended(&mut self, tc: TestCase) {
        let session = tc.draw(gs::sampled_from(SESSIONS.to_vec()));
        self.turns.queue_ended(session.to_be_bytes());
        let _new = self.ended.insert(session);
    }

    #[rule]
    fn turn(&mut self, tc: TestCase) {
        let checks_max = tc.draw(gs::integers::<usize>().min_value(1).max_value(4));
        let ended_max = tc.draw(gs::integers::<usize>().min_value(1).max_value(4));
        let turn = self.turns.take(checks_max, ended_max);
        // The model: the lowest sessions of the Endeds, and of the sessions with a check queued, each its first check.
        let ended: Vec<[u8; 16]> = self
            .ended
            .iter()
            .take(ended_max)
            .map(|session| session.to_be_bytes())
            .collect();
        for session in &ended {
            let _gone = self.ended.remove(&u128::from_be_bytes(*session));
        }
        let mut checks = Vec::new();
        for (session, queue) in self.queued.iter_mut().take(checks_max) {
            if let Some((ticket, kind)) = queue.pop_front() {
                checks.push((
                    ticket,
                    SessionCheck::new(session.to_be_bytes(), ALICE, kind),
                ));
            }
        }
        self.queued.retain(|_, queue| !queue.is_empty());
        assert_eq!(turn.ended(), ended.as_slice());
        assert_eq!(turn.checks(), checks.as_slice());
        self.sent
            .extend(turn.checks().iter().map(|(ticket, _)| *ticket));
    }

    #[invariant]
    fn agrees(&mut self, _tc: TestCase) {
        // Every ticket is sent at most once, and what is not sent is still queued.
        let sent: BTreeSet<u64> = self.sent.iter().copied().collect();
        assert_eq!(sent.len(), self.sent.len(), "a check sent twice");
        let waiting: usize = self.queued.values().map(VecDeque::len).sum();
        assert_eq!(sent.len() + waiting, self.tickets.len());
        assert_eq!(self.turns.is_empty(), waiting == 0 && self.ended.is_empty());
    }
}

/// L5.5, L5.6: any sequence of checks, Endeds and turns sends at most one Ended frame and one Check frame a turn, each
/// session at most once and ascending, within the frame's count, every check exactly once and a session's in their order.
#[hegel::test(test_cases = 60)]
fn test_a_turn_agrees_with_its_model(tc: TestCase) {
    let turns = Turns {
        turns: SystemTurns::new(),
        queued: BTreeMap::new(),
        ended: BTreeSet::new(),
        sent: Vec::new(),
        tickets: Vec::new(),
    };
    hegel::stateful::machine(turns).steps(40).run(tc);
}
