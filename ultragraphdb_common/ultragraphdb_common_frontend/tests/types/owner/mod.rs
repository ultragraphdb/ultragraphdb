/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The owner run on the test's runtime over the simulated storage, by a clock the test moves: what one turn takes, it takes.

#[cfg(test)]
mod owner_answer_tests;
#[cfg(test)]
mod owner_catalog_tests;
#[cfg(test)]
mod owner_check_tests;
#[cfg(test)]
mod owner_turn_tests;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compio::runtime::{JoinHandle, spawn};
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_sequencer::{ClockReading, SequencerConfig};
use ultragraphdb_common_frontend::{Owner, OwnerHandle, OwnerStart, owner_run};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_protocol_wire::ErrorReply;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_log::{SimStorage, Wal, WalConfig};

pub(crate) const CLUSTER: u64 = 7;

/// An epoch's length past its longest: moving the clock by it makes a close due.
pub(crate) const EPOCH_US: u64 = 10_000;

/// The owner over the simulated storage.
pub(crate) type SimOwner = Owner<Wal<SimStorage>, Wal<SimStorage>>;

/// A clock the test moves: the owner reads it, and only the test changes it.
#[derive(Debug, Clone)]
pub(crate) struct TestClock(Rc<Cell<u64>>);

impl TestClock {
    pub(crate) fn new() -> Self {
        Self(Rc::new(Cell::new(1)))
    }

    /// What the owner reads.
    pub(crate) fn reading(&self) -> impl Fn() -> ClockReading + 'static {
        let held = self.0.clone();
        move || ClockReading::new(held.get(), held.get())
    }

    pub(crate) fn now(&self) -> ClockReading {
        ClockReading::new(self.0.get(), self.0.get())
    }

    /// Moves the clock on by `us`.
    pub(crate) fn advance(&self, us: u64) {
        self.0.set(self.0.get().saturating_add(us));
    }
}

/// The most turns of the clock a [`ticking`] task makes: 5 s of them.
const TICKS_MAX: u32 = 5_000;

/// A task moving the clock on by an epoch every millisecond, for an answer that waits on closes; dropping it stops it.
pub(crate) fn ticking(clock: &TestClock) -> JoinHandle<()> {
    let clock = clock.clone();
    spawn(async move {
        for _ in 0..TICKS_MAX {
            clock.advance(EPOCH_US);
            compio::time::sleep(Duration::from_millis(1)).await;
        }
    })
}

/// The settings of an owner placing one partition, epochs of 1 to 5 ms.
pub(crate) fn starting(metadata: MetadataConfig) -> TestResult<OwnerStart> {
    Ok(OwnerStart::new(metadata, SequencerConfig::new(1, 5)?, 1))
}

/// The owner recovered over the logs `storage` holds, at the clock's reading.
pub(crate) async fn recovered(
    storage: &SimStorage,
    start: &OwnerStart,
    clock: &TestClock,
) -> TestResult<SimOwner> {
    let (metadata, _) = Wal::open(storage.clone(), "/meta", WalConfig::new(CLUSTER, false)).await?;
    let (sequencer, _) = Wal::open(storage.clone(), "/seq", WalConfig::new(CLUSTER, false)).await?;
    Ok(Owner::recover(metadata, sequencer, start, None, clock.now()).await?)
}

/// `owner` run on this runtime: the handle to ask it and its run, which ends once every handle is gone.
pub(crate) fn running(
    owner: SimOwner,
    clock: &TestClock,
) -> (OwnerHandle, JoinHandle<Result<(), RoleError>>) {
    let (handle, run) = owner_run(owner, clock.reading());
    (handle, spawn(run))
}

/// What the run ended with.
pub(crate) async fn ended(
    run: JoinHandle<Result<(), RoleError>>,
) -> TestResult<Result<(), RoleError>> {
    run.await
        .map_err(|error| TestError::new(format!("the owner's run: {error:?}")))
}

/// An answer the owner gave, a refusal failing the test.
pub(crate) fn heard<T>(answer: Result<T, ErrorReply>) -> TestResult<T> {
    answer.map_err(|refusal| TestError::new(refusal.message().to_string()))
}

/// A refusal the owner gave, an answer failing the test.
pub(crate) fn refused<T: std::fmt::Debug>(answer: Result<T, ErrorReply>) -> TestResult<ErrorReply> {
    match answer {
        Ok(answered) => Err(TestError::new(format!("answered {answered:?}"))),
        Err(refusal) => Ok(refusal),
    }
}

/// The next close after `after`, the clock moved on until it is due: its epoch.
pub(crate) async fn closed_after(
    handle: &OwnerHandle,
    clock: &TestClock,
    after: u64,
) -> TestResult<u64> {
    clock.advance(EPOCH_US);
    Ok(heard(handle.next_close(after).await)?.epoch().get())
}

/// The principal `name` created on `owner`, with the password `pw`.
pub(crate) async fn with_principal(owner: &mut SimOwner, name: &str) -> TestResult {
    let verifier = ScramVerifier::derive("pw", b"salt", 4_096)?;
    Ok(owner.create_principal_once(name, verifier).await?)
}

/// The identity of the principal `name`.
pub(crate) async fn principal(handle: &OwnerHandle, name: &str) -> TestResult<u64> {
    Ok(handle
        .verifier(name.to_string())
        .await
        .context("the principal")?
        .0)
}
