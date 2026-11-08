/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::future::Future;

use futures::channel::mpsc;
use ultragraphdb_cluster_sequencer::ClockReading;
use ultragraphdb_error_role::RoleError;
use ultragraphdb_traits_log::OrderedLog;

use crate::types::owner::Owner;
use crate::types::owner_handle::OwnerHandle;

/// `owner` run on the test's own runtime over the logs the test chose, by `clock`: the handle to ask it, and its run, which ends
/// once every handle is gone. Asks sent before the run is next polled are taken in one turn, so a test orders a turn exactly.
pub fn owner_run<M, S>(
    owner: Owner<M, S>,
    clock: impl Fn() -> ClockReading,
) -> (OwnerHandle, impl Future<Output = Result<(), RoleError>>)
where
    M: OrderedLog,
    S: OrderedLog,
{
    let (inbox, received) = mpsc::unbounded();
    (OwnerHandle::new(inbox), owner.run(received, clock))
}
