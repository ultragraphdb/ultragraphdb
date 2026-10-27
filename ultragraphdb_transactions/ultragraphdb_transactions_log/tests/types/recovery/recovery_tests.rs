/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_log::Position;
use ultragraphdb_transactions_log::{Damage, Recovery, Truncation};

#[test]
fn test_fields_read_back_as_given() {
    let truncation = Truncation::new(Position::new(5), 2, 8_192, Damage::Header);
    let recovery = Recovery::new(Position::new(3), Position::new(4), Some(truncation));
    assert_eq!(recovery.checkpoint(), Position::new(3));
    assert_eq!(recovery.committed(), Position::new(4));
    assert_eq!(recovery.truncation(), Some(truncation));
    assert_eq!(
        Recovery::new(Position::ZERO, Position::ZERO, None).truncation(),
        None
    );
}
