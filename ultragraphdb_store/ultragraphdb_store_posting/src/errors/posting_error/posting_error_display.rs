/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt::{Display, Formatter, Result};

use ultragraphdb_common_utils::go_strconv;
use ultragraphdb_error_store::go_hex_dump;

use super::{PostingError, PostingErrorEnum};

impl Display for PostingError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match &self.0 {
            PostingErrorEnum::Store(error) => write!(f, "{error}"),
            PostingErrorEnum::Key(error) => write!(f, "{error}"),
            PostingErrorEnum::InvalidKey => {
                f.write_str("cannot read posting list using multi-part list key")
            }
            PostingErrorEnum::NoValue => f.write_str("No value found"),
            PostingErrorEnum::TsTooOld => f.write_str("Transaction is too old"),
            // list.go: "readTs: %d less than minTs: %d for key: %q".
            PostingErrorEnum::ReadBelowMinTs {
                read_ts,
                min_ts,
                key,
            } => write!(
                f,
                "readTs: {read_ts} less than minTs: {min_ts} for key: {}",
                go_strconv::quote(key)
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
                go_hex_dump(key)
            ),
            PostingErrorEnum::UnexpectedMeta { meta, key } => {
                write!(f, "Unexpected meta: {meta} for key: {}", go_hex_dump(key))
            }
            // Badger's ErrKeyNotFound, wrapped as readListPart wraps it.
            PostingErrorEnum::MissingPart { key } => {
                write!(
                    f,
                    "could not read list part with key {}: Key not found",
                    hex(key)
                )
            }
            PostingErrorEnum::UnhandledOperation { op } => write!(f, "Unhandled operation: {op}"),
            PostingErrorEnum::BlockSize { actual, expected } => write!(
                f,
                "actual block size {actual} is different from expected value {expected}"
            ),
            PostingErrorEnum::HighPriorityOp => {
                f.write_str("Cancelled rollup to make way for high priority operation")
            }
        }
    }
}

/// Go's `hex.EncodeToString`.
fn hex(key: &[u8]) -> String {
    key.iter().map(|b| format!("{b:02x}")).collect()
}
