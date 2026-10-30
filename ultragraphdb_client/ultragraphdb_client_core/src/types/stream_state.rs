/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::VecDeque;

use ultragraphdb_protocol_transport::ReceivedFrame;

use crate::errors::client_error::ClientError;

/// One open stream of the connection: the request waiting on it for its one reply, the transaction it carries, the
/// result being read on it with its frames arrived and its credit, the transaction a read's stream runs in, and the
/// refusal it ended with.
#[derive(Debug, Default)]
pub(crate) struct StreamState {
    pub pending: Option<u64>,
    pub transaction: Option<bool>,
    pub results: bool,
    pub inbox: VecDeque<ReceivedFrame>,
    pub credit: u32,
    pub of: Option<u64>,
    pub failed: Option<ClientError>,
}
