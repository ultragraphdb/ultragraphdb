/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

// Arithmetic on decoded log input is checked: an overflow here would be a malformed input
// accepted with a wrapped value, so it is refused instead (user decision 2026-11-12; the lint is not in the
// workspace table because elsewhere it flags provably bounded index math).
#![deny(clippy::arithmetic_side_effects)]

use crate::errors::config_error::ConfigError;

const REVISION: u8 = 1;
const REVISION_ACTIVE: u8 = 2;
const ACTIVATE: u8 = 3;
const RETIRE: u8 = 4;

/// The bytes before a target's name: a kind and the name's length.
const HEAD: usize = 3;

/// One durable change, a log round's body: a kind byte, the target's name after its 2-byte little-endian length, a revision's
/// 8-byte little-endian number, and for a revision its files as they were received, to the end of the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigRecord<'a> {
    /// Stores `files` as the target's revision `number`, and, when `activate`, makes it active in the same step.
    Revision {
        /// The target.
        target: &'a str,
        /// The revision's number.
        number: u64,
        /// Whether the record also activates it.
        activate: bool,
        /// The revision's files, as received.
        files: &'a [u8],
    },
    /// Makes the target's revision `number` active, which ends a retirement.
    Activate {
        /// The target.
        target: &'a str,
        /// The revision's number.
        number: u64,
    },
    /// Retires the target.
    Retire {
        /// The target.
        target: &'a str,
    },
}

impl<'a> ConfigRecord<'a> {
    /// The record as a round's body.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let (kind, target, number, files): (u8, &str, u64, &[u8]) = match *self {
            Self::Revision {
                target,
                number,
                activate,
                files,
            } => (
                if activate { REVISION_ACTIVE } else { REVISION },
                target,
                number,
                files,
            ),
            Self::Activate { target, number } => (ACTIVATE, target, number, &[]),
            Self::Retire { target } => (RETIRE, target, 0, &[]),
        };
        // A capacity hint: the head, the name, the number and the files.
        let capacity = [target.len(), 8, files.len()]
            .iter()
            .fold(HEAD, |sum, len| sum.saturating_add(*len));
        let mut body = Vec::with_capacity(capacity);
        body.push(kind);
        body.extend_from_slice(
            &u16::try_from(target.len())
                .unwrap_or(u16::MAX)
                .to_le_bytes(),
        );
        body.extend_from_slice(target.as_bytes());
        body.extend_from_slice(&number.to_le_bytes());
        body.extend_from_slice(files);
        body
    }

    /// The record `body` holds, borrowing from it.
    ///
    /// # Errors
    ///
    /// [`ConfigError::malformed_record`] for a kind no record has, a body shorter than its fields, a name that is not
    /// Unicode, a number of 0, or bytes past the end of a record that holds no files.
    pub fn decode(body: &'a [u8]) -> Result<Self, ConfigError> {
        let short = || ConfigError::malformed_record("the body ends before its fields");
        let (&kind, rest) = body.split_first().ok_or_else(short)?;
        let (length, rest) = rest.split_first_chunk::<2>().ok_or_else(short)?;
        let (name, rest) = rest
            .split_at_checked(usize::from(u16::from_le_bytes(*length)))
            .ok_or_else(short)?;
        let target = std::str::from_utf8(name)
            .map_err(|_| ConfigError::malformed_record("the target's name is not Unicode"))?;
        let (number, files) = rest.split_first_chunk::<8>().ok_or_else(short)?;
        let number = u64::from_le_bytes(*number);
        let numbered = |record: Self| {
            if number == 0 {
                Err(ConfigError::malformed_record("a revision's number is 0"))
            } else {
                Ok(record)
            }
        };
        if kind != REVISION && kind != REVISION_ACTIVE && !files.is_empty() {
            return Err(ConfigError::malformed_record(
                "bytes follow a record that holds no files",
            ));
        }
        match kind {
            REVISION | REVISION_ACTIVE => numbered(Self::Revision {
                target,
                number,
                activate: kind == REVISION_ACTIVE,
                files,
            }),
            ACTIVATE => numbered(Self::Activate { target, number }),
            RETIRE if number == 0 => Ok(Self::Retire { target }),
            RETIRE => Err(ConfigError::malformed_record(
                "a retirement names a revision",
            )),
            other => Err(ConfigError::malformed_record(format!(
                "no record has the kind {other}"
            ))),
        }
    }
}
