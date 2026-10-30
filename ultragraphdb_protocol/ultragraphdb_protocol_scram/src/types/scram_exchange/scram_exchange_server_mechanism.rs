/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_transport::{Credential, MechanismStep, ServerMechanism};

use crate::errors::scram_error::ScramError;
use crate::traits::credentials::Credentials;
use crate::types::scram_exchange::ScramExchange;

impl<C: Credentials> ServerMechanism for ScramExchange<C> {
    type Error = ScramError;

    /// The client's first message answered with the server's first, its final one with the server's final message
    /// and the principal's credential, which never expires.
    fn step(&mut self, message: &[u8]) -> Result<MechanismStep, ScramError> {
        if !self.started {
            self.started = true;
            let credentials = &self.credentials;
            let first = self
                .server
                .first(message, |name| credentials.lookup(name))?;
            return Ok(MechanismStep::Continue(first));
        }
        let (last, principal) = self.server.last(message)?;
        Ok(MechanismStep::Done(last, Credential::new(principal)))
    }
}
