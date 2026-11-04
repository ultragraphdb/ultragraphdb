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
}

impl OwnerThread {
    /// The way to the owner.
    #[must_use]
    pub fn handle(&self) -> &OwnerHandle {
        &self.handle
    }

    /// Waits for the owner to stop: at a failure, or once every handle is gone, this one with it.
    ///
    /// # Errors
    ///
    /// What the owner stopped with.
    pub async fn stopped(self) -> Result<(), RoleError> {
        let Self { handle, stopped } = self;
        drop(handle);
        stopped.await.unwrap_or_else(|_| {
            Err(RoleError::software(
                "the owner's thread ended without a word",
            ))
        })
    }

    /// The handle to clone for each core, and what the owner stopped with, for a role that keeps both.
    pub(crate) fn into_parts(self) -> (OwnerHandle, oneshot::Receiver<Result<(), RoleError>>) {
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
        let wall = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_micros() as u64);
        ClockReading::new(self.started.elapsed().as_micros() as u64, wall)
    }
}

/// Starts the owner on a thread and runtime of its own, over logs under `data_dir`, and returns once it has recovered.
/// `principals`, each a name and its verifier, are created when the group does not hold them.
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
                            drop(recovered.send(Ok(())));
                            owner.run(received, move || clock.read()).await
                        }
                        Err(error) => {
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
    recovery.await.unwrap_or_else(|_| {
        Err(RoleError::software(
            "the owner's thread ended without a word",
        ))
    })?;
    Ok(OwnerThread {
        handle: OwnerHandle::new(inbox),
        stopped,
    })
}

async fn open(
    cluster: u64,
    data_dir: &std::path::Path,
    start: &OwnerStart,
    principals: Vec<(String, ScramVerifier)>,
    now: ClockReading,
) -> Result<Owner<Wal<CompioStorage>, Wal<CompioStorage>>, RoleError> {
    let failed = |what: &str, error: &dyn std::fmt::Display| {
        RoleError::io(format!("{what} under {}: {error}", data_dir.display()))
    };
    std::fs::create_dir_all(data_dir).map_err(|error| failed("the data directory", &error))?;
    // No drive here is assumed to keep what it acknowledged through a power loss.
    let (metadata, _) = Wal::open(
        CompioStorage,
        data_dir.join("metadata"),
        WalConfig::new(cluster, false),
    )
    .await
    .map_err(|error| failed("the metadata group's log", &error))?;
    let (sequencer, _) = Wal::open(
        CompioStorage,
        data_dir.join("sequencer"),
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
    Ok(owner)
}
