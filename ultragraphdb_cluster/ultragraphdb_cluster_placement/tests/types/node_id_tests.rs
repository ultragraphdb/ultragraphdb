/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::NodeId;

#[test]
fn test_a_node_holds_its_value() {
    assert_eq!(NodeId::new(0).get(), 0);
    assert_eq!(NodeId::new(7).get(), 7);
    assert_eq!(NodeId::new(u32::MAX).get(), u32::MAX);
    assert_ne!(NodeId::new(1), NodeId::new(2));
}
