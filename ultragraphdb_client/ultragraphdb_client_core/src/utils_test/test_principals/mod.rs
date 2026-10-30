/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod test_principals_credentials;

/// The name the test server's one principal authenticates with.
pub const TEST_NAME: &str = "alice";
/// That principal's password.
pub const TEST_PASSWORD: &str = "pencil";
/// That principal's number.
pub const TEST_PRINCIPAL: u64 = 7;

/// The test server's credentials: [`TEST_NAME`] alone, its verifier derived at the least iteration count so
/// an exchange costs little.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TestPrincipals;
