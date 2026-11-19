/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The stage of the system group's scenarios: the group run on the scenario's runtime over the simulated storage, by a clock
//! the scenario moves, and served at a role address over the TCP loopback, one `SystemSession` per connection.

use std::cell::Cell;
use std::future::Future;
use std::pin::pin;
use std::rc::Rc;
use std::time::Duration;

use compio::runtime::{JoinHandle, spawn};
use futures::future::poll_immediate;
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_sequencer::{ClockReading, SequencerConfig};
use ultragraphdb_common_role::{
    ROLE_ITERATIONS, RoleClient, RoleCluster, RoleSecret, Server, serve_connection,
};
use ultragraphdb_common_system::{
    SystemGroup, SystemHandle, SystemSession, SystemShared, SystemStart, system_run,
};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_protocol_transport::{Address, Endpoint, ReceivedFrame};
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_protocol_wire::{BodyView, ErrorCode, ErrorReply, ErrorReplyView};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_log::{SimStorage, Wal, WalConfig};

pub(crate) const CLUSTER: u64 = 7;

/// An epoch's length past its longest at 1 to 5 ms: moving the clock by it makes a close due.
pub(crate) const EPOCH_US: u64 = 10_000;

/// `transaction_ms` of the scenarios.
pub(crate) const T_MS: u64 = 50;

/// Twice `transaction_ms`, in microseconds.
pub(crate) const HOLD_US: u64 = 2 * T_MS * 1_000;

/// How long a scenario waits for a frame it expects.
pub(crate) const WAITED: Duration = Duration::from_secs(5);

/// The most closes a wait on the group makes before it fails: a bound on a hang, past what any scenario waits for.
const CLOSES_MAX: u32 = 1_000;

/// The group over the simulated storage.
pub(crate) type SimGroup = SystemGroup<Wal<SimStorage>, Wal<SimStorage>>;

/// A clock the scenario moves: the group reads it, and only the scenario changes it.
#[derive(Debug, Clone)]
pub(crate) struct Clock(Rc<Cell<u64>>);

impl Clock {
    pub(crate) fn new() -> Self {
        Self(Rc::new(Cell::new(1)))
    }

    pub(crate) fn reading(&self) -> impl Fn() -> ClockReading + 'static {
        let held = self.0.clone();
        move || ClockReading::new(held.get(), held.get())
    }

    pub(crate) fn now(&self) -> ClockReading {
        ClockReading::new(self.0.get(), self.0.get())
    }

    pub(crate) fn advance(&self, us: u64) {
        self.0.set(self.0.get().saturating_add(us));
    }
}

/// The settings of the scenarios' group: epochs of 1 to 5 ms over `partitions`, a registry of `sessions_max`.
pub(crate) fn start_of(partitions: u32, sessions_max: u32) -> TestResult<SystemStart> {
    Ok(SystemStart::new(
        MetadataConfig::new().with_sessions_max(sessions_max)?,
        SequencerConfig::new(1, 5)?,
        partitions,
    )
    .with_transaction_ms(T_MS))
}

/// The group recovered over the logs `storage` holds, with `start`, at `clock`'s reading, holding `alice`, `bob` and `ann`, each
/// with the password `pw`.
pub(crate) async fn recovered(
    storage: &SimStorage,
    start: &SystemStart,
    clock: &Clock,
) -> TestResult<SimGroup> {
    let (metadata, _) = Wal::open(storage.clone(), "/meta", WalConfig::new(CLUSTER, false)).await?;
    let (sequencer, _) = Wal::open(storage.clone(), "/seq", WalConfig::new(CLUSTER, false)).await?;
    let mut group = SystemGroup::recover(metadata, sequencer, start, None, clock.now()).await?;
    for tenant in start.tenants() {
        group.create_tenant_once(*tenant).await?;
    }
    for name in ["alice", "bob", "ann"] {
        let verifier = ScramVerifier::derive("pw", format!("a salt of {name}").as_bytes(), 4_096)?;
        group.create_principal_once(name, verifier).await?;
    }
    Ok(group)
}

/// A group running on this runtime, served at a role address.
pub(crate) struct Stage {
    pub(crate) storage: SimStorage,
    pub(crate) clock: Clock,
    pub(crate) handle: SystemHandle,
    pub(crate) run: JoinHandle<Result<(), RoleError>>,
    pub(crate) address: Address,
    pub(crate) cluster: RoleCluster,
    /// The task accepting role connections, whose sessions hold a way to the group: dropped, it accepts no more.
    accepting: JoinHandle<()>,
}

impl Stage {
    /// The group of `start` over a fresh simulated storage seeded `seed`, run and served.
    pub(crate) async fn new(seed: u64, start: &SystemStart) -> TestResult<Self> {
        let storage = SimStorage::new(seed);
        let clock = Clock::new();
        let group = recovered(&storage, start, &clock).await?;
        Self::serve(storage, clock, group).await
    }

    /// `group`, over `storage` by `clock`, run and served.
    pub(crate) async fn serve(
        storage: SimStorage,
        clock: Clock,
        group: SimGroup,
    ) -> TestResult<Self> {
        let (handle, run) = system_run(group, clock.reading());
        let run = spawn(run);
        let root = std::env::var_os("TEST_TMPDIR")
            .map_or_else(std::env::temp_dir, std::path::PathBuf::from);
        let file = root.join(format!("system-bdd-secret-{}", std::process::id()));
        std::fs::write(&file, "a role secret")?;
        let secret = RoleSecret::read(&file.display().to_string())?;
        let cluster = RoleCluster::new(CLUSTER, secret, Duration::from_secs(1));
        let shared = SystemShared::new(cluster.clone(), handle.clone());
        // Bound at port 0 and read back, so no other process takes the port between a choice and the bind.
        let listener = Listener::bind(&Address::any_port("127.0.0.1")?).await?;
        let address = listener.local_address()?;
        let server =
            Server::new(CLUSTER, "frontend", Endpoint::System)?.with_iterations(ROLE_ITERATIONS);
        let credentials = cluster.secret().credentials()?;
        let accepting = spawn(async move {
            while let Ok(carrier) = listener.accept().await {
                let (server, credentials) = (server.clone(), credentials.clone());
                let session = SystemSession::new(shared.clone());
                spawn(
                    async move { serve_connection(carrier, &server, credentials, session).await },
                )
                .detach();
            }
        });
        Ok(Self {
            storage,
            clock,
            handle,
            run,
            address,
            cluster,
            accepting,
        })
    }

    /// A role connection dialled as `role`.
    pub(crate) async fn dial(&self, role: &str) -> TestResult<RoleClient> {
        Ok(RoleClient::dial(
            &self.address,
            CLUSTER,
            role,
            self.cluster.secret().password(),
        )
        .await?)
    }

    /// A role connection dialled as `role`, stating `max_frame` in its Hello.
    pub(crate) async fn dial_taking(&self, role: &str, max_frame: u32) -> TestResult<RoleClient> {
        Ok(RoleClient::dial_taking(
            &self.address,
            CLUSTER,
            role,
            self.cluster.secret().password(),
            max_frame,
        )
        .await?)
    }

    /// The principal named `name`.
    pub(crate) async fn principal(&self, name: &str) -> TestResult<u64> {
        Ok(self
            .handle
            .verifier(name.to_string())
            .await
            .context("the principal")?
            .0)
    }

    /// The group stopped: no connection accepted any more, every handle dropped once the connections closed, and its run
    /// ended.
    pub(crate) async fn stop(self) -> TestResult<(SimStorage, Clock)> {
        let Self {
            storage,
            clock,
            handle,
            run,
            accepting,
            ..
        } = self;
        drop(accepting);
        drop(handle);
        run.await
            .map_err(|error| TestError::new(format!("the group's run: {error:?}")))??;
        Ok((storage, clock))
    }

    /// The group's host killed: its run dropped where it stands, what its logs hold unsynced durable or lost as a process's end
    /// leaves it, and every ask waiting on it answered as the group stopped.
    pub(crate) fn kill(self) -> (SimStorage, Clock) {
        let Self {
            storage,
            clock,
            handle,
            run,
            accepting,
            ..
        } = self;
        drop(accepting);
        drop(handle);
        drop(run);
        (storage, clock)
    }

    /// The next close after `after`, the clock moved on until it is due: its epoch.
    pub(crate) async fn close_after(&self, after: u64) -> TestResult<u64> {
        self.clock.advance(EPOCH_US);
        Ok(heard(self.handle.next_close(after).await)?.epoch().get())
    }

    /// What `asked` answers, the clock moved on by an epoch each time the group has answered all it can without a close, and
    /// that close delivered before `asked` is polled again: the closes it waits on, made by the scenario and not by real
    /// time. The group answers a `LatestClose` as it takes it, keeping one that comes during a sync for the next turn, so
    /// the second of two, sent once the first is answered, is answered once a turn after every ask `asked` sent has ended.
    pub(crate) async fn closing<T>(&self, asked: impl Future<Output = T>) -> TestResult<T> {
        let mut asked = pin!(asked);
        for _ in 0..CLOSES_MAX {
            if let Some(answer) = poll_immediate(asked.as_mut()).await {
                return Ok(answer);
            }
            let _taken = heard(self.handle.latest_close().await)?;
            let after = heard(self.handle.latest_close().await)?;
            if let Some(answer) = poll_immediate(asked.as_mut()).await {
                return Ok(answer);
            }
            let _closed = self.close_after(after).await?;
        }
        Err(TestError::new(format!(
            "not answered within {CLOSES_MAX} closes"
        )))
    }
}

/// An answer the group gave, a refusal failing the scenario.
pub(crate) fn heard<T>(answer: Result<T, ErrorReply>) -> TestResult<T> {
    answer.map_err(|refusal| TestError::new(refusal.message().to_string()))
}

/// A refusal the group gave, an answer failing the scenario.
pub(crate) fn refused<T: std::fmt::Debug>(answer: Result<T, ErrorReply>) -> TestResult<ErrorReply> {
    match answer {
        Ok(answered) => Err(TestError::new(format!("answered {answered:?}"))),
        Err(refusal) => Ok(refusal),
    }
}

/// The next frame on `client`, within [`WAITED`].
pub(crate) async fn next_frame(client: &mut RoleClient) -> TestResult<ReceivedFrame> {
    compio::time::timeout(WAITED, client.next_frame())
        .await
        .map_err(|_| TestError::new("no frame came"))?
        .map_err(TestError::from)
}

/// The Error `frame` is: its stream, code and message; `None` for another command.
pub(crate) fn error_of(frame: &ReceivedFrame) -> TestResult<Option<(u64, ErrorCode, String)>> {
    let view = frame.view()?;
    if view.command() != ultragraphdb_protocol_wire::Command::Error {
        return Ok(None);
    }
    let error = ErrorReplyView::from_frame(&view)?;
    Ok(Some((
        view.header().stream(),
        error.code(),
        error.message().to_string(),
    )))
}
