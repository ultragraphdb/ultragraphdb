/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_traits_log::{DurabilityTicket, Position};

#[test]
fn test_a_ticket_names_the_position_it_was_made_for() -> TestResult {
    // The first, an inner and the last position: a getter returning a constant passes at most one.
    for n in [1, 7, u64::MAX] {
        assert_eq!(
            DurabilityTicket::new(Position::new(n)).position(),
            Position::new(n)
        );
    }
    Ok(())
}

#[test]
fn test_tickets_order_as_their_positions() -> TestResult {
    let early = DurabilityTicket::new(Position::new(3));
    let late = DurabilityTicket::new(Position::new(4));
    assert!(early < late);
    assert_ne!(early, late);
    assert_eq!(early, DurabilityTicket::new(Position::new(3)));
    Ok(())
}
