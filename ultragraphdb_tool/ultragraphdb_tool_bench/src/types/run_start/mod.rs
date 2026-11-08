/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::mpsc::{Receiver, SendError, Sender, channel};

use crate::types::run_start_party::RunStartParty;
use crate::types::wall_clock::WallClock;

/// The start of a run over several threads: each thread's [`RunStartParty`] says when its setup is done, and the run's
/// clock starts once every party has said so or has gone, then reaches each. A setup, such as opening a log, then never
/// counts against the run's length, however long the machine makes it take.
#[derive(Debug)]
pub struct RunStart {
    ready: Receiver<()>,
    clocks: Vec<Sender<WallClock>>,
}

impl RunStart {
    /// The start of a run of `parties` threads, and each thread's side of it.
    #[must_use]
    pub fn new(parties: usize) -> (Self, Vec<RunStartParty>) {
        let (ready, ready_rx) = channel();
        let (clocks, waiting): (Vec<_>, Vec<_>) = (0..parties)
            .map(|_| {
                let (clock, clock_rx) = channel();
                (clock, RunStartParty::new(ready.clone(), clock_rx))
            })
            .unzip();
        // Only the parties hold a sender now, so a party gone ends the wait for it.
        drop(ready);
        (
            Self {
                ready: ready_rx,
                clocks,
            },
            waiting,
        )
    }

    /// Waits until every party is ready or gone, then starts the run's clock, hands it to each party and returns it.
    #[must_use]
    pub fn start(self) -> WallClock {
        // Ends at the last party ready, or once every party not yet ready has gone, each dropping its sender.
        for _ in 0..self.clocks.len() {
            if self.ready.recv().is_err() {
                break;
            }
        }
        let clock = WallClock::new();
        for party in &self.clocks {
            // A party gone has failed; its thread's result says why.
            let _gone: Result<(), SendError<WallClock>> = party.send(clock);
        }
        clock
    }
}
