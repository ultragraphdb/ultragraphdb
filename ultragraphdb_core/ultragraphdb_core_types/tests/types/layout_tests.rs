/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! go-geom's `Layout` (geom.go): `Stride` and `String`.

use ultragraphdb_core_types::Layout;

#[test]
fn test_layout_values_strides_and_names() {
    let cases = [
        (Layout::NO_LAYOUT, 0, 0, "NoLayout"),
        (Layout::XY, 1, 2, "XY"),
        (Layout::XYZ, 2, 3, "XYZ"),
        (Layout::XYM, 3, 3, "XYM"),
        (Layout::XYZM, 4, 4, "XYZM"),
        (Layout::new(5), 5, 5, "Layout(5)"),
        (Layout::new(9), 9, 9, "Layout(9)"),
        (
            Layout::new(u32::MAX),
            u32::MAX,
            u32::MAX as usize,
            "Layout(4294967295)",
        ),
    ];
    for (layout, value, stride, name) in cases {
        assert_eq!(layout.value(), value, "{name}");
        assert_eq!(Layout::new(value), layout, "{name}");
        assert_eq!(layout.stride(), stride, "{name}");
        assert_eq!(layout.name(), name, "{value}");
    }
}
