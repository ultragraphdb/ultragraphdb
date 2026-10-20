/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_types::Length;

#[test]
fn test_length_display() {
    assert_eq!(Length::new(1500.0).to_string(), "1.500 km");
    assert_eq!(Length::new(0.5).to_string(), "50.000 cm");
    assert_eq!(Length::new(12.0).to_string(), "12.000 m");
}
