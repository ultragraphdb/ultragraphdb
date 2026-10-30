/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::convert::Infallible;

use crate::traits::client_mechanism::ClientMechanism;
use crate::types::anonymous::Anonymous;

/// The mechanism type of [`Anonymous`], which its login never gives: every message is empty.
impl ClientMechanism for Anonymous {
    type Error = Infallible;

    fn name(&self) -> &str {
        ""
    }

    fn start(&mut self) -> Result<Vec<u8>, Self::Error> {
        Ok(Vec::new())
    }

    fn answer(&mut self, _challenge: &[u8]) -> Result<Vec<u8>, Self::Error> {
        Ok(Vec::new())
    }

    fn finish(&mut self, _outcome: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }
}
