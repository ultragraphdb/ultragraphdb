/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::scram_error::ScramError;
use crate::utils::message::encode;

/// The random bytes of a nonce: 144 bits, 24 characters of base64.
pub const NONCE_BYTES: usize = 18;

/// A nonce from the operating system's random source: [`NONCE_BYTES`] bytes in standard base64, which holds no comma,
/// so it serves a client's nonce and a server's alike; [`ScramError::Random`] when the source fails.
pub fn system_nonce() -> Result<String, ScramError> {
    let mut bytes = [0u8; NONCE_BYTES];
    getrandom::fill(&mut bytes).map_err(|error| ScramError::Random(error.to_string()))?;
    Ok(encode(&bytes))
}
