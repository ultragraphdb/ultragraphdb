/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::credential::Credential;

/// A server mechanism's answer to a client message: its next message, or its final one and the credential it
/// established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MechanismStep {
    /// The server's message; the exchange goes on.
    Continue(Vec<u8>),
    /// The server's final message and what the exchange established.
    Done(Vec<u8>, Credential),
}
