/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::VecDeque;
use std::time::Instant;

use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{Command, FLAG_LAST};

use crate::errors::client_error::ClientError;

/// One open stream of the connection: the request waiting on it for its one reply, the transaction it carries, the
/// result being read on it with its frames arrived and its credit, the transaction a read's stream runs in, the refusal
/// it ended with, a read's last progress, and the transaction's reads' deadline.
#[derive(Debug, Default)]
pub(crate) struct StreamState {
    pub pending: Option<u64>,
    pub transaction: Option<bool>,
    pub results: bool,
    pub inbox: VecDeque<ReceivedFrame>,
    pub credit: u32,
    pub of: Option<u64>,
    pub failed: Option<ClientError>,
    pub progress: Option<Instant>,
    pub deadline: Option<Instant>,
    /// Whether the caller holds the read's last batch: it waits on its caller until it asks for the next one.
    pub with_caller: bool,
}

impl StreamState {
    /// Whether the read's frames hold its end: its `LAST` or a refusal, which every Error is, waiting to be read. The
    /// stream has ended, so nothing more is sent for it.
    pub(crate) fn ended(&self) -> bool {
        self.inbox.back().is_some_and(|frame| {
            frame.flags() & FLAG_LAST != 0 || frame.command() == Command::Error.number()
        })
    }
}
