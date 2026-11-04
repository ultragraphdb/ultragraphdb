/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_service::Readiness;

/// A role starts not ready, and a clone sees what the original sets, as the responder sees the role.
#[test]
fn test_readiness_is_shared_and_starts_false() {
    let role = Readiness::default();
    let responder = role.clone();
    assert!(!responder.is_ready());
    role.set(true);
    assert!(responder.is_ready());
    role.set(false);
    assert!(!responder.is_ready());
}
