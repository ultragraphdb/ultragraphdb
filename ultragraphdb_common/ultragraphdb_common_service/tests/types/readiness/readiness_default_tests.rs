/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_service::Readiness;

/// A role of the default readiness listens from its start and is not ready.
#[test]
fn test_a_default_readiness_listens_and_is_not_ready() {
    let role = Readiness::default();
    assert!(role.is_listening());
    assert!(!role.is_ready());
}
