/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bench::Size;

#[test]
fn test_sizes_add_each_measure_apart() {
    let sum = Size::new(10, 4_096).plus(Size::new(5, 512));
    assert_eq!((sum.apparent(), sum.allocated()), (15, 4_608));
    assert_eq!(Size::default(), Size::new(0, 0));
    assert_eq!(Size::new(3, 7).plus(Size::default()), Size::new(3, 7));
}
