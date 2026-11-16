/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The owner run on the test's runtime over the simulated storage, by a clock the test moves: what one turn takes, it takes.

#[cfg(test)]
mod system_group_answer_tests;
#[cfg(test)]
mod system_group_catalog_tests;
#[cfg(test)]
mod system_group_check_tests;
#[cfg(test)]
mod system_group_reads_tests;
#[cfg(test)]
pub(crate) mod system_group_sessions_tests;
#[cfg(test)]
mod system_group_syncing_tests;
#[cfg(test)]
mod system_group_turn_tests;

use std::cell::Cell;
use std::future::Future;
use std::pin::{Pin, pin};
use std::rc::Rc;

use compio::runtime::{JoinHandle, spawn};
use futures::future::poll_immediate;
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_sequencer::{ClockReading, SequencerConfig};
use ultragraphdb_common_system::{SystemGroup, SystemHandle, SystemStart, system_run};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_protocol_wire::ErrorReply;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_log::{SimStorage, Wal, WalConfig};

pub(crate) const CLUSTER: u64 = 7;

/// An epoch's length past its longest: moving the clock by it makes a close due.
pub(crate) const EPOCH_US: u64 = 10_000;

/// The owner over the simulated storage.
pub(crate) type SimOwner = SystemGroup<Wal<SimStorage>, Wal<SimStorage>>;

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

/// The most closes a wait on the owner makes before it fails: a bound on a hang, past what any test here waits for.
const CLOSES_MAX: u32 = 1_000;

/// The latest close, read once the owner has ended a turn that began after every ask sent before this one: whatever of those
/// asks a turn answers without a close is answered. The owner answers a `LatestClose` as it takes it, keeping one that comes
/// during a sync for the next turn, so the second, sent once the first is answered, is taken in a turn after the first's.
pub(crate) async fn turned(handle: &SystemHandle) -> TestResult<u64> {
    let _taken = heard(handle.latest_close().await)?;
    heard(handle.latest_close().await)
}

/// What `asked` answers, the clock moved on by an epoch each time the owner has answered all it can without a close, and
/// that close delivered before `asked` is polled again: the closes it waits on, made by the test and not by real time.
pub(crate) async fn closing<T>(
    handle: &SystemHandle,
    clock: &TestClock,
    asked: impl Future<Output = T>,
) -> TestResult<T> {
    let mut asked = pin!(asked);
    for _ in 0..CLOSES_MAX {
        if let Some(answer) = poll_immediate(asked.as_mut()).await {
            return Ok(answer);
        }
        let after = turned(handle).await?;
        if let Some(answer) = poll_immediate(asked.as_mut()).await {
            return Ok(answer);
        }
        let _closed = closed_after(handle, clock, after).await?;
    }
    Err(TestError::new(format!(
        "not answered within {CLOSES_MAX} closes"
    )))
}

/// `asked` polled once, which sends its ask: the owner, not run by that poll, has answered nothing of it yet.
pub(crate) async fn sent<F: Future<Output: std::fmt::Debug>>(asked: Pin<&mut F>) -> TestResult {
    match poll_immediate(asked).await {
        None => Ok(()),
        Some(answer) => Err(TestError::new(format!(
            "answered before the owner ran: {answer:?}"
        ))),
    }
}

/// The first `Some` that `read` answers, read again after each close the clock is moved on to.
pub(crate) async fn closed_until<T, Read, Answer>(
    handle: &SystemHandle,
    clock: &TestClock,
    mut read: Read,
) -> TestResult<T>
where
    Read: FnMut() -> Answer,
    Answer: Future<Output = TestResult<Option<T>>>,
{
    for _ in 0..CLOSES_MAX {
        if let Some(found) = read().await? {
            return Ok(found);
        }
        let after = heard(handle.latest_close().await)?;
        let _closed = closed_after(handle, clock, after).await?;
    }
    Err(TestError::new(format!(
        "never came to hold within {CLOSES_MAX} closes"
    )))
}

/// The settings of an owner placing one partition, epochs of 1 to 5 ms.
pub(crate) fn starting(metadata: MetadataConfig) -> TestResult<SystemStart> {
    Ok(SystemStart::new(metadata, SequencerConfig::new(1, 5)?, 1))
}

/// The owner recovered over the logs `storage` holds, at the clock's reading.
pub(crate) async fn recovered(
    storage: &SimStorage,
    start: &SystemStart,
    clock: &TestClock,
) -> TestResult<SimOwner> {
    let (metadata, _) = Wal::open(storage.clone(), "/meta", WalConfig::new(CLUSTER, false)).await?;
    let (sequencer, _) = Wal::open(storage.clone(), "/seq", WalConfig::new(CLUSTER, false)).await?;
    let mut owner = SystemGroup::recover(metadata, sequencer, start, None, clock.now()).await?;
    // The tenants beside tenant 0 a start names, as `start_system` creates them.
    for tenant in start.tenants() {
        owner.create_tenant_once(*tenant).await?;
    }
    Ok(owner)
}

/// `owner` run on this runtime: the handle to ask it and its run, which ends once every handle is gone.
pub(crate) fn running(
    owner: SimOwner,
    clock: &TestClock,
) -> (SystemHandle, JoinHandle<Result<(), RoleError>>) {
    let (handle, run) = system_run(owner, clock.reading());
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
    handle: &SystemHandle,
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
pub(crate) async fn principal(handle: &SystemHandle, name: &str) -> TestResult<u64> {
    Ok(handle
        .verifier(name.to_string())
        .await
        .context("the principal")?
        .0)
}
