/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::txn_error::{TxnError, TxnErrorEnum};

impl fmt::Display for TxnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            TxnErrorEnum::ReadAboveWatermark { version, watermark } => write!(
                f,
                "read version {version} is above the visibility watermark {watermark}"
            ),
            TxnErrorEnum::PastLimit { limit_ms } if limit_ms % 1_000 == 0 => {
                write!(f, "transaction past its limit of {} s", limit_ms / 1_000)
            }
            TxnErrorEnum::PastLimit { limit_ms } => {
                write!(f, "transaction past its limit of {limit_ms} ms")
            }
            TxnErrorEnum::WriteSetTooLarge { size, limit } => write!(
                f,
                "write set of {size} bytes exceeds the limit of {limit} bytes"
            ),
            TxnErrorEnum::ReadSetTooLarge { size, limit } => write!(
                f,
                "read set of {size} bytes exceeds the limit of {limit} bytes"
            ),
            TxnErrorEnum::ReadOnly => write!(f, "a read-only transaction does not write"),
            TxnErrorEnum::Namespace {
                transaction,
                changeset,
            } => write!(
                f,
                "a changeset of namespace {changeset} in a transaction of namespace {transaction}"
            ),
            TxnErrorEnum::Ended => write!(f, "the transaction has ended"),
            TxnErrorEnum::Changeset(e) => e.fmt(f),
        }
    }
}
