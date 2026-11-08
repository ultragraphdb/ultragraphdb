/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::mpsc::{Receiver, SendError, Sender};

use crate::errors::bench_error::BenchError;
use crate::types::wall_clock::WallClock;

/// One thread's side of a [`crate::RunStart`]: it says when its setup is done, then takes the run's clock.
#[derive(Debug)]
pub struct RunStartParty {
    /// Present until the party has said it is ready.
    ready: Option<Sender<()>>,
    clock: Receiver<WallClock>,
}

impl RunStartParty {
    pub(crate) fn new(ready: Sender<()>, clock: Receiver<WallClock>) -> Self {
        Self {
            ready: Some(ready),
            clock,
        }
    }

    /// Says the party's setup is done; once, whatever the calls.
    pub fn ready(&mut self) {
        if let Some(ready) = self.ready.take() {
            // The start gone, the clock never comes, which `clock` reports.
            let _unheard: Result<(), SendError<()>> = ready.send(());
        }
    }

    /// Says the party is ready, unless it has, and waits for the run's clock.
    ///
    /// # Errors
    ///
    /// [`BenchError::commit`] when the run's start was dropped without starting.
    pub fn clock(mut self) -> Result<WallClock, BenchError> {
        self.ready();
        self.clock
            .recv()
            .map_err(|_| BenchError::commit("the run ended before it started".to_string()))
    }
}
