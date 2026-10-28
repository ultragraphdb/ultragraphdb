/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::txn_error::{TxnError, TxnErrorEnum};

impl std::error::Error for TxnError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.0 {
            TxnErrorEnum::Changeset(e) => Some(e),
            _ => None,
        }
    }
}
