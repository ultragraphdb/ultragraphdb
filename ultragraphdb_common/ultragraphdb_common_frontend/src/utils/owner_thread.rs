/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use futures::channel::{mpsc, oneshot};
use ultragraphdb_cluster_sequencer::ClockReading;
use ultragraphdb_common_service::core_runtime;
use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_transactions_log::{CompioStorage, Wal, WalConfig};

use crate::types::owner::Owner;
use crate::types::owner_handle::OwnerHandle;
use crate::types::owner_start::OwnerStart;

/// The owner's core, where the endpoint's cores are numbered from 0.
const OWNER_CORE: usize = 1;

/// A running owner: the way to it, and what it stopped with.
#[derive(Debug)]
pub struct OwnerThread {
    handle: OwnerHandle,
    stopped: oneshot::Receiver<Result<(), RoleError>>,
    unknown_secret: [u8; 32],
}

impl OwnerThread {
    /// The way to the owner.
    #[must_use]
    pub fn handle(&self) -> &OwnerHandle {
        &self.handle
    }

    /// The secret an unknown name's verifier derives from, which the metadata group holds durably.
    #[must_use]
    pub fn unknown_secret(&self) -> [u8; 32] {
        self.unknown_secret
    }

    /// Waits for the owner to stop: at a failure, or once every handle is gone, this one with it.
    ///
    /// A dropped future has let go of this handle already, and leaves the owner running until its other handles are
    /// gone, its outcome unheard.
    ///
    /// # Errors
    ///
    /// What the owner stopped with.
    pub async fn stopped(self) -> Result<(), RoleError> {
        let Self {
            handle, stopped, ..
        } = self;
        drop(handle);
        stopped.await.unwrap_or_else(|_| {
            Err(RoleError::software(
                "the owner's thread ended without a word",
            ))
        })
    }

    /// The handle to clone for each core, and what the owner stopped with, for a role that keeps both.
    pub fn into_parts(self) -> (OwnerHandle, oneshot::Receiver<Result<(), RoleError>>) {
        (self.handle, self.stopped)
    }
}

/// The clock the owner tells its state machines, which read none.
#[derive(Debug, Clone, Copy)]
struct OwnerClock {
    started: Instant,
}

impl OwnerClock {
    #[expect(
        clippy::disallowed_methods,
        reason = "the shell reads the clock and tells the owner's state machines, which read none"
    )]
    fn new() -> Self {
        Self {
            started: Instant::now(),
        }
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "the shell reads the clock and tells the owner's state machines, which read none"
    )]
    fn read(self) -> ClockReading {
        // A `u128` of microseconds fits a `u64` for 584,000 years, past what the elapsed time or a wall clock reaches;
        // a reading past that saturates.
        let wall = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| {
                u64::try_from(since.as_micros()).unwrap_or(u64::MAX)
            });
        let elapsed = u64::try_from(self.started.elapsed().as_micros()).unwrap_or(u64::MAX);
        ClockReading::new(elapsed, wall)
    }
}

/// Starts the owner on a thread and runtime of its own, over logs under `data_dir`, and returns once it has recovered.
/// `principals`, each a name and its verifier, are created when the group does not hold them.
///
/// A future dropped while it waits for recovery drops the one handle, so the owner's thread recovers, finds no handle
/// left and stops.
///
/// # Errors
///
/// What recovery refused, [`RoleError::unavailable`] when the thread cannot start.
pub async fn start_owner(
    config: ServiceConfig,
    cluster: u64,
    data_dir: PathBuf,
    start: OwnerStart,
    principals: Vec<(String, ScramVerifier)>,
) -> Result<OwnerThread, RoleError> {
    // Bounded as `OwnerHandle` says: each connection waits for its answer before it asks again.
    let (inbox, received) = mpsc::unbounded();
    let (recovered, recovery) = oneshot::channel();
    let (stop, stopped) = oneshot::channel();
    #[expect(
        clippy::disallowed_methods,
        reason = "the owner is a real core, which no simulation replays: its state machines are simulated beneath it"
    )]
    let spawned = std::thread::Builder::new()
        .name("frontend-owner".to_string())
        .spawn(move || {
            let outcome = core_runtime(&config, OWNER_CORE).and_then(|(runtime, cpu)| {
                tracing::info!(core = "owner", "{cpu}");
                runtime.block_on(async move {
                    let clock = OwnerClock::new();
                    let opened = open(cluster, &data_dir, &start, principals, clock.read()).await;
                    match opened {
                        Ok(owner) => {
                            // A role gone before its owner recovered has nothing left to run for.
                            drop(recovered.send(Ok(owner.unknown_secret().unwrap_or_default())));
                            owner.run(received, move || clock.read()).await
                        }
                        Err(error) => {
                            // A role gone has no one to tell, and the thread's outcome carries the error too.
                            drop(recovered.send(Err(error.clone())));
                            Err(error)
                        }
                    }
                })
            });
            // A role gone has no one to tell.
            drop(stop.send(outcome));
        });
    let _thread = spawned.map_err(|error| RoleError::unavailable(format!("the owner: {error}")))?;
    let unknown_secret = recovery.await.unwrap_or_else(|_| {
        Err(RoleError::software(
            "the owner's thread ended without a word",
        ))
    })?;
    Ok(OwnerThread {
        handle: OwnerHandle::new(inbox),
        stopped,
        unknown_secret,
    })
}

/// A future dropped at a log's open or the recovery leaves the logs as they were; at a principal's or a tenant's
/// creation, as [`Owner::create_principal_once`] says.
async fn open(
    cluster: u64,
    data_dir: &std::path::Path,
    start: &OwnerStart,
    principals: Vec<(String, ScramVerifier)>,
    now: ClockReading,
) -> Result<Owner<Wal<CompioStorage>, Wal<CompioStorage>>, RoleError> {
    // A closure takes no type parameter, so it takes each error as a trait object (Ultra rules 1 and 9).
    let failed = |what: &str, error: &dyn std::fmt::Display| {
        RoleError::io(format!("{what} under {}: {error}", data_dir.display()))
    };
    std::fs::create_dir_all(data_dir).map_err(|error| failed("the data directory", &error))?;
    // No drive here is assumed to keep what it acknowledged through a power loss.
    let (metadata, _) = Wal::open(
        CompioStorage,
        data_dir.join("meta"),
        WalConfig::new(cluster, false),
    )
    .await
    .map_err(|error| failed("the metadata group's log", &error))?;
    let (sequencer, _) = Wal::open(
        CompioStorage,
        data_dir.join("seq"),
        WalConfig::new(cluster, false),
    )
    .await
    .map_err(|error| failed("the sequencer's log", &error))?;
    let mut owner = Owner::recover(
        metadata,
        sequencer,
        start,
        Some(data_dir.to_path_buf()),
        now,
    )
    .await?;
    for (name, verifier) in principals {
        owner.create_principal_once(&name, verifier).await?;
    }
    for tenant in start.tenants() {
        owner.create_tenant_once(*tenant).await?;
    }
    Ok(owner)
}
