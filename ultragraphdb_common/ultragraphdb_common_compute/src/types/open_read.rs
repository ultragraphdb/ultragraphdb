/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::runtime::JoinHandle;
use futures::channel::mpsc::UnboundedSender;
use ultragraphdb_common_role::OpenFrame;

/// A read under way on a connection: its task, whose end cancels what it holds, the signal of room for its next batch, and its
/// count among the requests a stop waits on.
#[derive(Debug)]
pub(super) struct OpenRead {
    pub(crate) _task: JoinHandle<()>,
    pub(crate) room: UnboundedSender<()>,
    pub(crate) _open: OpenFrame,
}
