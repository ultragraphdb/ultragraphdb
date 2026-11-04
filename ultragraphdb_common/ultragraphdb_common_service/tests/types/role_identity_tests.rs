/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_service::RoleIdentity;

/// An identity reads back what it was made of, and states all of it in one line.
#[test]
fn test_an_identity_reads_back_and_displays() {
    let identity = RoleIdentity::new("ci", "storage_1", 7);
    assert_eq!(identity.target(), "ci");
    assert_eq!(identity.instance(), "storage_1");
    assert_eq!(identity.revision(), 7);
    assert_eq!(
        identity.to_string(),
        "target ci, instance storage_1, revision 7"
    );
}
