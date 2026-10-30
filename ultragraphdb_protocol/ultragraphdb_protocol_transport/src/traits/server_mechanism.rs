/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt::Display;

use crate::types::mechanism_step::MechanismStep;

/// A server's side of one SASL exchange: each client message answered, until the exchange ends with the credential
/// it established. A refusal is answered with code 19 and closes the connection, saying nothing of why; the error is
/// the server's, for its audit.
pub trait ServerMechanism {
    /// Why the exchange failed.
    type Error: Display;

    /// The answer to the client's `message`.
    fn step(&mut self, message: &[u8]) -> Result<MechanismStep, Self::Error>;
}
