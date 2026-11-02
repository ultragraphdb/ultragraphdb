/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::id_error::{IdError, IdErrorEnum};

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            IdErrorEnum::Length {
                what,
                expected,
                found,
            } => write!(f, "{what} is {expected} bytes long, not {found}"),
            IdErrorEnum::ReservedBits { bits } => write!(
                f,
                "invalid system ID: its reserved bits are 0x{bits:06X}, not zero"
            ),
            IdErrorEnum::ZeroTypePrefix => write!(f, "invalid system ID: its type prefix is 0"),
            IdErrorEnum::ZeroSequence => write!(f, "invalid system ID: its sequence is 0"),
            IdErrorEnum::BucketRange { bucket } => write!(
                f,
                "bucket {bucket} does not fit in 24 bits; the largest is 16777215"
            ),
            IdErrorEnum::IriTooLong { len } => write!(
                f,
                "an IRI of {len} bytes is longer than a canonical key's 4-byte length counts, {} bytes",
                u32::MAX
            ),
            IdErrorEnum::EpochRange { epoch } => write!(
                f,
                "epoch {epoch} does not fit in 42 bits; the largest is 4398046511103"
            ),
            IdErrorEnum::CommitSequenceRange { sequence } => write!(
                f,
                "commit sequence {sequence} does not fit in 22 bits as an even number; the largest is 4194302"
            ),
            IdErrorEnum::OddCommitSequence { sequence } => write!(
                f,
                "commit sequence {sequence} is odd; commit versions are even"
            ),
        }
    }
}
