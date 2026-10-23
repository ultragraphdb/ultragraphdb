/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_types::Area;

#[test]
fn test_area_display() {
    assert_eq!(Area::new(2.5e6).to_string(), "2.500 km^2");
    assert_eq!(Area::new(0.5).to_string(), "5000.000 cm^2");
    assert_eq!(Area::new(12.0).to_string(), "12.000 m^2");
}
