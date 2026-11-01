/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::client_error::{ClientError, ClientErrorEnum};

impl std::error::Error for ClientError {
    /// What gave a lost connection up, for [`ClientErrorEnum::ConnectionLost`] with a cause.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.0 {
            ClientErrorEnum::ConnectionLost { cause: Some(cause) } => Some(cause.as_ref()),
            _ => None,
        }
    }
}
