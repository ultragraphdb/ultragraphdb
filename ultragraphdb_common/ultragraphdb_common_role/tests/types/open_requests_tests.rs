/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_role::OpenRequests;
use ultragraphdb_common_service::Stopping;

#[test]
fn test_requests_start_with_none_open_and_follow_their_stopping() {
    let stopping = Stopping::default();
    let open = OpenRequests::new(stopping.clone());
    assert_eq!(open.count(), 0);
    assert!(!open.is_stopping());
    stopping.set();
    assert!(open.is_stopping());
    assert!(!OpenRequests::default().is_stopping());
}
