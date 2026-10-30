/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod test_authenticator_authenticator;

use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

/// The test server's authenticator: SCRAM-SHA-256 over [`TestPrincipals`], each exchange's server nonce
/// numbered, every credential lasting `life` from the exchange's end when one is given, by the server's wall clock
/// `clock`, microseconds of Unix time.
#[derive(Debug, Clone)]
pub struct TestAuthenticator {
    clock: Arc<AtomicU64>,
    life: Option<Duration>,
    exchanges: u64,
}

impl TestAuthenticator {
    pub fn new(clock: Arc<AtomicU64>, life: Option<Duration>) -> Self {
        Self {
            clock,
            life,
            exchanges: 0,
        }
    }
}
