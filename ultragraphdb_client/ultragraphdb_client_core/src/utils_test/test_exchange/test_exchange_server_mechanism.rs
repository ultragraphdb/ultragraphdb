/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::atomic::Ordering;

use ultragraphdb_protocol_scram::ScramExchange;
use ultragraphdb_protocol_transport::{Credential, MechanismStep, ServerMechanism};

use crate::utils_test::test_exchange::TestExchange;
use crate::utils_test::test_principals::TestPrincipals;

impl ServerMechanism for TestExchange {
    type Error = <ScramExchange<TestPrincipals> as ServerMechanism>::Error;

    fn step(&mut self, message: &[u8]) -> Result<MechanismStep, Self::Error> {
        Ok(match (self.inner.step(message)?, self.life) {
            (MechanismStep::Done(message, credential), Some(life)) => {
                let now = self.clock.load(Ordering::Relaxed);
                let expires = now.saturating_add(life.as_micros() as u64);
                MechanismStep::Done(
                    message,
                    Credential::expiring(credential.principal(), expires, life),
                )
            }
            (step, _) => step,
        })
    }
}
