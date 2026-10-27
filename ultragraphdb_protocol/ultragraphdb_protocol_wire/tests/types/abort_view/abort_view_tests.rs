/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{Abort, AbortView, BodyView};

use crate::utils::golden_bodies::{frame_of, golden};

#[test]
fn test_the_golden_abort_reads() {
    let bytes = golden("abort").bytes();
    assert_eq!(
        AbortView::from_frame(&frame_of(&bytes))
            .unwrap()
            .to_body()
            .unwrap(),
        Abort
    );
}
