/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::log_error::{LogError, LogErrorEnum};

impl fmt::Display for LogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            LogErrorEnum::NotPrimary => write!(f, "this replica is not the primary"),
            LogErrorEnum::Busy { segments } => write!(
                f,
                "the log holds its capacity of {segments} segments; a checkpoint must trim it first"
            ),
            LogErrorEnum::Sealed { position } => {
                write!(f, "the log was sealed at position {position}")
            }
            LogErrorEnum::Truncated { position } => write!(
                f,
                "round {position} was replaced before it became durable; its outcome is unknown"
            ),
            LogErrorEnum::PositionMismatch { expected, found } => {
                write!(f, "position {found} is not the expected {expected}")
            }
            LogErrorEnum::ParentMismatch {
                position,
                expected,
                found,
            } => write!(
                f,
                "round {position} names parent {found:#018x}, not the previous round's checksum {expected:#018x}"
            ),
            LogErrorEnum::TooLarge { size, limit } => {
                write!(f, "round of {size} bytes exceeds the {limit}-byte limit")
            }
            LogErrorEnum::Corrupt {
                position,
                segment,
                offset,
            } => write!(
                f,
                "round {position} at segment {segment} byte {offset} is damaged, and a complete round follows it"
            ),
            LogErrorEnum::CorruptChunk {
                position,
                segment,
                offset,
                chunk,
                start,
                end,
            } => write!(
                f,
                "chunk {chunk}, body bytes {start}..{end}, of round {position} at segment {segment} byte {offset} is damaged, and a complete round follows it"
            ),
            LogErrorEnum::SuperblockUnreadable { copy } => write!(
                f,
                "superblock copy {copy} does not verify; the log is not started afresh over it"
            ),
            LogErrorEnum::Diverged {
                what,
                expected,
                found,
            } => write!(f, "the log diverged: {what} is {found}, not {expected}"),
            LogErrorEnum::Io {
                operation,
                kind,
                message,
            } => write!(f, "storage {operation} failed ({kind}): {message}"),
            LogErrorEnum::Poisoned => write!(
                f,
                "the log is poisoned by an earlier failed write or sync; open it again"
            ),
        }
    }
}
