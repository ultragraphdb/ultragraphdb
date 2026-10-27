/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_log::{LogEvent, Position};

// LogEvent is a plain enum: these tests pin that its variants carry what they name, for the VSR implementation and
// the sequencer that will match on them.
#[test]
fn test_events_carry_their_values() {
    let changed = LogEvent::ViewChanged { view: 3 };
    let replaced = LogEvent::SuffixReplaced {
        from: Position::new(12),
    };
    assert_ne!(changed, replaced);
    assert!(matches!(changed, LogEvent::ViewChanged { view: 3 }));
    assert!(matches!(replaced, LogEvent::SuffixReplaced { from } if from == Position::new(12)));
}
