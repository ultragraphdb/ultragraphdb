/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_service::RoleKind;

#[test]
fn test_a_role_is_logged_by_its_name() {
    assert_eq!(RoleKind::Frontend.to_string(), "frontend");
    assert_eq!(RoleKind::Compute.to_string(), "compute");
    assert_eq!(RoleKind::Storage.to_string(), "storage");
}
