/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod test_exchange_server_mechanism;

use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use ultragraphdb_protocol_scram::ScramExchange;

use crate::utils_test::test_principals::TestPrincipals;

/// One exchange of the test server: SCRAM's, its credential given the server's credential life.
#[derive(Debug, Clone)]
pub struct TestExchange {
    inner: ScramExchange<TestPrincipals>,
    clock: Arc<AtomicU64>,
    life: Option<Duration>,
}

impl TestExchange {
    pub fn new(
        inner: ScramExchange<TestPrincipals>,
        clock: Arc<AtomicU64>,
        life: Option<Duration>,
    ) -> Self {
        Self { inner, clock, life }
    }
}
