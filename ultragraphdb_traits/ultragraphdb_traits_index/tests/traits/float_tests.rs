/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_index::Float;

#[test]
fn test_float_converts_as_go_does() {
    assert_eq!(f32::from_f32(1.5), 1.5f32);
    assert_eq!(f32::from_f64(0.1), 0.1f64 as f32);
    assert_eq!(f64::from_f32(0.1), f64::from(0.1f32));
    assert_eq!(f64::from_f64(0.1), 0.1);
}
