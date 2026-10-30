/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_scram::{DEFAULT_ITERATIONS, MECHANISM, MIN_ITERATIONS, SALT_SIZE};

#[test]
fn test_the_constants_are_the_specs() {
    assert_eq!(MECHANISM, "SCRAM-SHA-256");
    // RFC 7677 section 4; SECURITY.md's OWASP count.
    assert_eq!(MIN_ITERATIONS, 4_096);
    assert_eq!(DEFAULT_ITERATIONS, 600_000);
    assert_eq!(SALT_SIZE, 16);
}
