/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod syncer_drop;

use std::sync::Weak;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use ultragraphdb_error_store::StoreError;

use crate::types::fjall_store::Shared;

/// The thread that syncs a store in [`crate::Durability::Process`] mode once a minute, as Dgraph's `x.StoreSync`
/// does (`x/x.go:1281-1297`). It holds the store weakly and stops when the store is dropped.
#[derive(Debug)]
pub struct Syncer {
    stop: Option<Sender<()>>,
    handle: Option<JoinHandle<()>>,
}

impl Syncer {
    /// The period of Dgraph's `x.StoreSync`.
    pub const PERIOD: Duration = Duration::from_secs(60);

    #[allow(
        clippy::disallowed_methods,
        reason = "the production store syncs on a thread of its own, as Dgraph's StoreSync goroutine does; the \
                  simulated store replaces it under the simulator"
    )]
    pub fn start(shared: Weak<Shared>, period: Duration) -> Result<Self, StoreError> {
        let (stop, stopped) = mpsc::channel::<()>();
        let handle = thread::Builder::new()
            .name("ultragraphdb-store-sync".to_string())
            .spawn(move || {
                while let Err(RecvTimeoutError::Timeout) = stopped.recv_timeout(period) {
                    let Some(shared) = shared.upgrade() else {
                        return;
                    };
                    shared.periodic_sync();
                }
            })
            .map_err(|e| StoreError::OpenFailed(format!("starting the sync thread: {e}")))?;
        Ok(Self {
            stop: Some(stop),
            handle: Some(handle),
        })
    }
}
