/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_compute::prefix_end;

#[test]
fn test_the_key_past_a_prefix_raises_its_last_byte() {
    assert_eq!(prefix_end(&[1, 2, 3]), [1, 2, 4]);
}

#[test]
fn test_the_key_past_a_prefix_drops_what_cannot_rise() {
    assert_eq!(prefix_end(&[1, 2, 255, 255]), [1, 3]);
}

#[test]
fn test_no_key_lies_past_a_prefix_of_every_byte_at_its_top() {
    assert_eq!(prefix_end(&[255, 255]), Vec::<u8>::new());
}
