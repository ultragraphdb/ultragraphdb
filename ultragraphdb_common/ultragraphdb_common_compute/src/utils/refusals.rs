/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{ErrorCode, ErrorReply};

/// Code 17: the read found what compute cannot answer it through.
pub(crate) fn unavailable(message: impl Into<String>) -> ErrorReply {
    ErrorReply::new(ErrorCode::Unavailable, message)
}

/// The `detail` of codes 23 and 25: the transaction's or lowest partition in bits 0–31, the lowest other in bits 32–63.
pub(crate) fn partitions_detail(partition: u32, other: u32) -> u64 {
    u64::from(partition) | (u64::from(other) << 32)
}

/// Code 7 for the term of index `index` breaking `rule`.
pub(crate) fn term_refused(index: u32, rule: &str) -> ErrorReply {
    ErrorReply::new(ErrorCode::Refused, rule).with_detail(index.into())
}

/// Error 1 naming the resume key.
pub(crate) fn resume_refused(why: &dyn std::fmt::Display) -> ErrorReply {
    ErrorReply::new(ErrorCode::Protocol, format!("the resume key: {why}"))
}
