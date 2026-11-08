/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::ResumeRow;

#[test]
fn test_a_row_gives_back_each_position_it_was_built_with() {
    let named = ResumeRow::new(Some(2), 3, 4, 5, Some(6));
    assert_eq!(
        (
            named.graph(),
            named.subject(),
            named.predicate(),
            named.object(),
            named.reifier()
        ),
        (Some(2), 3, 4, 5, Some(6))
    );
    let default = ResumeRow::new(None, 7, 8, 9, None);
    assert_eq!(
        (
            default.graph(),
            default.subject(),
            default.predicate(),
            default.object(),
            default.reifier()
        ),
        (None, 7, 8, 9, None)
    );
}
