/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt::{Display, Formatter, Result};

use super::{PostingError, PostingErrorEnum};

impl Display for PostingError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match &self.0 {
            PostingErrorEnum::Store(error) => write!(f, "{error}"),
            PostingErrorEnum::NotAList { key } => {
                write!(f, "Key {} is not a posting list's", hex_dump(key))
            }
            PostingErrorEnum::InvalidKey => {
                f.write_str("cannot read posting list using multi-part list key")
            }
            PostingErrorEnum::TsTooOld => f.write_str("Transaction is too old"),
            PostingErrorEnum::ReadBelowMinTs {
                read_ts,
                min_ts,
                key,
            } => write!(
                f,
                "readTs: {read_ts} less than minTs: {min_ts} for key: \"{}\"",
                key.escape_ascii()
            ),
            PostingErrorEnum::DeleteBelowMinTs {
                delete_below_ts,
                min_ts,
            } => write!(
                f,
                "deleteBelowTs ({delete_below_ts}) must be greater than the minTs in the list ({min_ts})"
            ),
            PostingErrorEnum::SchemaPosting { key } => write!(
                f,
                "Trying to read schema in ReadPostingList for key: {}",
                hex_dump(key)
            ),
            PostingErrorEnum::UnexpectedMeta { meta, key } => {
                write!(f, "Unexpected meta: {meta} for key: {}", hex_dump(key))
            }
            // Badger's ErrKeyNotFound, wrapped as readListPart wraps it.
            PostingErrorEnum::MissingPart { key } => {
                write!(
                    f,
                    "could not read list part with key {}: Key not found",
                    hex(key)
                )
            }
            PostingErrorEnum::HighPriorityOp => {
                f.write_str("Cancelled rollup to make way for high priority operation")
            }
            PostingErrorEnum::StaleVersion { version, after } => {
                write!(f, "commit version {version} is not above {after}")
            }
        }
    }
}

/// Go's `hex.EncodeToString`.
fn hex(key: &[u8]) -> String {
    key.iter().map(|b| format!("{b:02x}")).collect()
}
