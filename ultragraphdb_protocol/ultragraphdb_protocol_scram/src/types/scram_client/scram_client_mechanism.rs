/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_transport::ClientMechanism;

use crate::errors::scram_error::ScramError;
use crate::types::scram_client::ScramClient;
use crate::utils::constants::MECHANISM;

impl ClientMechanism for ScramClient {
    type Error = ScramError;

    fn name(&self) -> &str {
        MECHANISM
    }

    fn start(&mut self) -> Result<Vec<u8>, ScramError> {
        self.first_message()
    }

    fn answer(&mut self, challenge: &[u8]) -> Result<Vec<u8>, ScramError> {
        self.final_message(challenge)
    }

    fn finish(&mut self, outcome: &[u8]) -> Result<(), ScramError> {
        self.verify(outcome)
    }
}
