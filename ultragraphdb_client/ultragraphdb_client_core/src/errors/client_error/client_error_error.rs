/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::client_error::{ClientError, ClientErrorEnum};

impl std::error::Error for ClientError {
    /// What gave a lost connection up, or left a commit in doubt, for an error with a cause.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.0 {
            ClientErrorEnum::ConnectionLost { cause: Some(cause) }
            | ClientErrorEnum::InDoubt {
                cause: Some(cause), ..
            } => Some(cause.as_ref()),
            ClientErrorEnum::ConnectionLost { cause: None }
            | ClientErrorEnum::InDoubt { cause: None, .. }
            | ClientErrorEnum::Transport { .. }
            | ClientErrorEnum::Protocol { .. }
            | ClientErrorEnum::Refused { .. }
            | ClientErrorEnum::Closed { .. }
            | ClientErrorEnum::TimedOut
            | ClientErrorEnum::NotConnected
            | ClientErrorEnum::Request { .. }
            | ClientErrorEnum::ReplyMismatch { .. }
            | ClientErrorEnum::OutcomeUnknown { .. }
            | ClientErrorEnum::NotInDoubt { .. }
            | ClientErrorEnum::SessionBusy { .. }
            | ClientErrorEnum::UnknownSession { .. }
            | ClientErrorEnum::ReadOnly { .. }
            | ClientErrorEnum::StreamFinished { .. }
            | ClientErrorEnum::StreamBusy { .. }
            | ClientErrorEnum::StreamLimit { .. }
            | ClientErrorEnum::BufferLimit { .. }
            | ClientErrorEnum::TransactionMismatch { .. } => None,
        }
    }
}
