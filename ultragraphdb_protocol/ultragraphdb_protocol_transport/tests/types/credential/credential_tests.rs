/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use ultragraphdb_protocol_transport::Credential;

#[test]
fn test_a_credential_keeps_its_principal_and_expiry() {
    let never = Credential::new(7);
    assert_eq!(
        (never.principal(), never.expires(), never.valid_for()),
        (7, 0, None)
    );
    let hour = Credential::expiring(8, 1_000, Duration::from_secs(3_600));
    assert_eq!(
        (hour.principal(), hour.expires(), hour.valid_for()),
        (8, 1_000, Some(Duration::from_secs(3_600)))
    );
    assert_ne!(never, Credential::new(8));
}
