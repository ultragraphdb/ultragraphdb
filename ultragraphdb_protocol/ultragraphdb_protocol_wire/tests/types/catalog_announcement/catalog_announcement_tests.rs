/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::CatalogAnnouncement;
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_an_announcement_encodes_namespace_then_version() -> TestResult {
    let announcement = CatalogAnnouncement::new(0x0102_0304_0506_0708, 0x1112_1314_1516_1718);
    assert_eq!(announcement.namespace(), 0x0102_0304_0506_0708);
    assert_eq!(announcement.version(), 0x1112_1314_1516_1718);
    assert_eq!(
        announcement.encode(),
        [
            8, 7, 6, 5, 4, 3, 2, 1, 0x18, 0x17, 0x16, 0x15, 0x14, 0x13, 0x12, 0x11
        ]
    );
    assert_eq!(CatalogAnnouncement::SIZE, 16);
    Ok(())
}
