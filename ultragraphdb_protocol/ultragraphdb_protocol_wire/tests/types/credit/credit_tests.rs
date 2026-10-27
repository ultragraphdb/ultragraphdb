/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Credit};

use crate::utils::golden_bodies::golden;

#[test]
fn test_credit_builds_the_golden_frame() {
    assert_eq!(Credit::new(2).frames(), 2);
    assert_eq!(
        Credit::new(2).frame(golden("credit").header()).unwrap(),
        golden("credit").bytes()
    );
}
