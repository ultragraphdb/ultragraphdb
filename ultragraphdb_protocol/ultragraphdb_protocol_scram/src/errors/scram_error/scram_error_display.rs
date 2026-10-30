/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::scram_error::{ScramError, ScramErrorEnum};

impl fmt::Display for ScramError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            ScramErrorEnum::Malformed { part } => {
                write!(f, "a SCRAM message's {part} does not parse")
            }
            ScramErrorEnum::ChannelBinding => {
                write!(f, "channel binding is not offered without TLS")
            }
            ScramErrorEnum::Prepare => write!(f, "SASLprep refuses the text"),
            ScramErrorEnum::Iterations { iterations } => write!(
                f,
                "the iteration count {iterations} is outside 4,096 to 10,000,000"
            ),
            ScramErrorEnum::Nonce => write!(f, "the server's nonce does not extend the client's"),
            ScramErrorEnum::Proof => write!(f, "the client's proof does not verify"),
            ScramErrorEnum::ServerSignature => write!(f, "the server's signature does not verify"),
            ScramErrorEnum::Server { reason } => write!(f, "the server refused: {reason}"),
            ScramErrorEnum::OutOfStep => write!(f, "a SCRAM message out of step"),
            ScramErrorEnum::Random { reason } => {
                write!(f, "the random source failed: {reason}")
            }
        }
    }
}
