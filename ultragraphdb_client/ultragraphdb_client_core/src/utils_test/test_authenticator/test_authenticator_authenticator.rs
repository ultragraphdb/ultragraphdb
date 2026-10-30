/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;

use ultragraphdb_protocol_scram::{MECHANISM, MIN_ITERATIONS, ScramExchange, ScramServer};
use ultragraphdb_protocol_transport::Authenticator;

use crate::utils_test::test_authenticator::TestAuthenticator;
use crate::utils_test::test_exchange::TestExchange;
use crate::utils_test::test_principals::TestPrincipals;

impl Authenticator for TestAuthenticator {
    type Mechanism = TestExchange;

    fn start(&mut self, mechanism: &str) -> Option<TestExchange> {
        if mechanism != MECHANISM {
            return None;
        }
        self.exchanges += 1;
        let nonce = format!("test-{}", self.exchanges);
        let server = ScramServer::new(&nonce, [5; 32]).with_iterations(MIN_ITERATIONS);
        Some(TestExchange::new(
            ScramExchange::new(server, TestPrincipals),
            Arc::clone(&self.clock),
            self.life,
        ))
    }
}
