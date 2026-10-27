/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyView, PatternSlot, ReadView};

use crate::types::read::read_tests::golden_read;
use crate::utils::golden_bodies::{frame_of, golden};

#[test]
fn test_the_golden_read_reads_its_values() {
    let bytes = golden("read").bytes();
    let view = ReadView::from_frame(&frame_of(&bytes)).unwrap();
    assert_eq!(view.namespace(), 0);
    assert_eq!(view.limit(), 100);
    assert_eq!(view.resume(), &[1, 2, 3]);
    assert_eq!(view.pattern().graph(), &PatternSlot::Unbound);
    assert_eq!(
        view.pattern().subject(),
        &PatternSlot::Bound("<http://ex.org/a>")
    );
    assert_eq!(
        view.pattern().predicate(),
        &PatternSlot::Bound("<http://ex.org/name>")
    );
    assert_eq!(view.pattern().object(), &PatternSlot::Unbound);
    assert_eq!(view.to_body().unwrap(), golden_read());
}
