/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::types::credential::Credential;

/// A server mechanism's answer to a client message: its next message, or its final one and the credential it
/// established. Its `Debug` holds the credential and never a message, which may carry a signature.
#[derive(Clone, PartialEq, Eq)]
pub enum MechanismStep {
    /// The server's message; the exchange goes on.
    Continue(Vec<u8>),
    /// The server's final message and what the exchange established.
    Done(Vec<u8>, Credential),
}

impl fmt::Debug for MechanismStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Continue(_) => f
                .debug_tuple("Continue")
                .field(&format_args!("<redacted>"))
                .finish(),
            Self::Done(_, credential) => f
                .debug_tuple("Done")
                .field(&format_args!("<redacted>"))
                .field(credential)
                .finish(),
        }
    }
}
