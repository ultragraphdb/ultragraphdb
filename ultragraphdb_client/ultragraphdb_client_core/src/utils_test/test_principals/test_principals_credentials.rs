/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_scram::{Credentials, MIN_ITERATIONS, ScramVerifier};

use crate::utils_test::test_principals::{
    TEST_NAME, TEST_PASSWORD, TEST_PRINCIPAL, TestPrincipals,
};

impl Credentials for TestPrincipals {
    fn lookup(&self, name: &str) -> Option<(u64, ScramVerifier)> {
        if name != TEST_NAME {
            return None;
        }
        ScramVerifier::derive(TEST_PASSWORD, b"test-salt", MIN_ITERATIONS)
            .ok()
            .map(|verifier| (TEST_PRINCIPAL, verifier))
    }
}
