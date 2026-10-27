/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyView, Execute, ExecuteView};

use crate::utils::golden_bodies::{frame_of, golden};

#[test]
fn test_the_golden_execute_reads_its_values() {
    let bytes = golden("execute").bytes();
    let view = ExecuteView::from_frame(&frame_of(&bytes)).unwrap();
    assert_eq!(view.statement(), 1);
    assert_eq!(view.limit(), 0);
    assert_eq!(view.parameters().get(0), Some("<http://ex.org/a>"));
    assert_eq!(view.parameters().len(), 1);
    assert!(view.resume().is_empty());
    assert_eq!(
        view.to_body().unwrap(),
        Execute::new(1, 0, vec!["<http://ex.org/a>".to_string()], Vec::new())
    );
}
