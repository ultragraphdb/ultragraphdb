/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::task::Waker;

use crate::types::network_faults::NetworkFaults;
use crate::types::network_stats::NetworkStats;
use crate::types::sim_stream::SimStream;
use crate::utils::split_mix::SplitMix;

/// One direction of a simulated connection: messages in flight with the step they arrive at, the chunks arrived and
/// not yet read, whether the writer has shut it down, and the reader waiting for it.
#[derive(Debug, Clone, Default)]
pub(crate) struct LinkState {
    pub(crate) in_flight: VecDeque<(u64, Vec<u8>)>,
    pub(crate) arrived: VecDeque<Vec<u8>>,
    pub(crate) closed: bool,
    pub(crate) reader: Option<Waker>,
}

/// The network's links, clock, faults and draws, shared by the network and its streams.
#[derive(Debug, Clone)]
pub(crate) struct NetworkState {
    pub(crate) links: Vec<LinkState>,
    pub(crate) now: u64,
    pub(crate) faults: NetworkFaults,
    pub(crate) rng: SplitMix,
    pub(crate) stats: NetworkStats,
}

impl NetworkState {
    /// Whether any message is in flight on any link.
    pub(crate) fn in_flight(&self) -> bool {
        self.links.iter().any(|link| !link.in_flight.is_empty())
    }

    /// Sends `message` on `link` at the current step, as the faults draw: dropped, or delivered once or twice, after
    /// one step or delayed. A link delivers from its front only, so a message never overtakes one sent before it.
    pub(crate) fn send(&mut self, link: usize, message: Vec<u8>) {
        self.stats.sent = self.stats.sent.saturating_add(1);
        if self.rng.chance(self.faults.drop_rate()) {
            self.stats.dropped = self.stats.dropped.saturating_add(1);
            return;
        }
        let copies = if self.rng.chance(self.faults.duplicate_rate()) {
            self.stats.duplicated = self.stats.duplicated.saturating_add(1);
            2
        } else {
            1
        };
        let mut due = self.now.saturating_add(1);
        if self.rng.chance(self.faults.delay_rate()) {
            self.stats.delayed = self.stats.delayed.saturating_add(1);
            due = due.saturating_add(self.rng.below(self.faults.max_delay()));
        }
        let Some(state) = self.links.get_mut(link) else {
            return;
        };
        for _ in 1..copies {
            state.in_flight.push_back((due, message.clone()));
        }
        state.in_flight.push_back((due, message));
        if let Some(reader) = state.reader.take() {
            reader.wake();
        }
    }

    /// Advances the clock one step: every message due arrives, split into chunks as the faults draw.
    pub(crate) fn step(&mut self) {
        self.now = self.now.saturating_add(1);
        for link in 0..self.links.len() {
            // Each turn takes one message off the link, and no message is sent during a step.
            let in_flight = self.links.get(link).map_or(0, |l| l.in_flight.len());
            for _ in 0..in_flight {
                let due = matches!(
                    self.links.get(link).and_then(|l| l.in_flight.front()),
                    Some((due, _)) if *due <= self.now
                );
                if !due {
                    break;
                }
                let Some((_, message)) = self
                    .links
                    .get_mut(link)
                    .and_then(|l| l.in_flight.pop_front())
                else {
                    break;
                };
                let chunks = if message.len() > 1 && self.rng.chance(self.faults.split_rate()) {
                    self.stats.split = self.stats.split.saturating_add(1);
                    self.split(&message)
                } else {
                    vec![message]
                };
                if let Some(state) = self.links.get_mut(link) {
                    state.arrived.extend(chunks);
                    if let Some(reader) = state.reader.take() {
                        reader.wake();
                    }
                }
            }
        }
    }
}

impl NetworkState {
    /// `message`, of at least two bytes, cut at one to three distinct points drawn in `1..len`, so no chunk is empty.
    fn split(&mut self, message: &[u8]) -> Vec<Vec<u8>> {
        debug_assert!(message.len() > 1);
        let len = message.len() as u64; // lossless: a `usize` is at most 64 bits
        // `below(3)` is under 3 and `below(len - 1)` under `len - 1`, so both casts are lossless.
        let pieces = 2 + self.rng.below(3) as usize;
        let mut cuts: Vec<usize> = (1..pieces)
            .map(|_| 1 + self.rng.below(len.saturating_sub(1)) as usize)
            .collect();
        cuts.sort_unstable();
        cuts.dedup();
        let mut chunks = Vec::with_capacity(pieces);
        let mut start = 0;
        for cut in cuts.into_iter().chain([message.len()]) {
            if let Some(chunk) = message.get(start..cut) {
                chunks.push(chunk.to_vec());
            }
            start = cut;
        }
        debug_assert_eq!(chunks.iter().map(Vec::len).sum::<usize>(), message.len());
        chunks
    }
}

/// A deterministic network of connections for the simulator. Each write to a [`SimStream`] is one message, which the
/// network may drop, duplicate, delay or split by its [`NetworkFaults`], drawing from one seeded stream; messages
/// arrive as the clock advances, by [`SimNetwork::step`] or by a read waiting for its link, so a seed replays its
/// run.
#[derive(Debug, Clone)]
pub struct SimNetwork {
    state: Rc<RefCell<NetworkState>>,
}

impl SimNetwork {
    /// A network with no connections, drawing its `faults` from `seed`.
    pub fn new(seed: u64, faults: NetworkFaults) -> Self {
        Self {
            state: Rc::new(RefCell::new(NetworkState {
                links: Vec::new(),
                now: 0,
                faults,
                rng: SplitMix::new(seed),
                stats: NetworkStats::default(),
            })),
        }
    }

    /// A connection: the client's end and the server's.
    pub fn connect(&self) -> (SimStream, SimStream) {
        let mut state = self.state.borrow_mut();
        let to_server = state.links.len();
        state.links.push(LinkState::default());
        state.links.push(LinkState::default());
        drop(state);
        (
            SimStream::new(Rc::clone(&self.state), to_server + 1, to_server),
            SimStream::new(Rc::clone(&self.state), to_server, to_server + 1),
        )
    }

    /// Advances the clock one step, delivering every message due.
    pub fn step(&self) {
        self.state.borrow_mut().step();
    }

    /// Steps until no message is in flight: at most one step past the longest delay, since every message is due within
    /// `max_delay` steps of the step it was sent at, and none is sent during a step.
    pub fn run_until_idle(&self) {
        let steps_max = self.state.borrow().faults.max_delay().saturating_add(1);
        for _ in 0..steps_max {
            if !self.state.borrow().in_flight() {
                return;
            }
            self.step();
        }
        debug_assert!(!self.state.borrow().in_flight());
    }

    /// The steps taken.
    pub fn now(&self) -> u64 {
        self.state.borrow().now
    }

    /// What the network did to the messages written so far.
    pub fn stats(&self) -> NetworkStats {
        self.state.borrow().stats
    }
}
