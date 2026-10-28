/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_transport::{ClientHandshake, ServerHandshake};
use ultragraphdb_protocol_wire::{Hello, Welcome};

const CLUSTER: u64 = 3;

#[test]
fn test_an_answer_gives_its_frame_and_outcome_and_parts_alike() {
    let hello = ClientHandshake::new(CLUSTER, Hello::new(1, 1, 0, "c"))
        .hello_frame()
        .unwrap();
    let answer =
        ServerHandshake::new(CLUSTER, Welcome::new(1, 0, 1 << 20, 1, 0, "s")).receive(&hello);
    let (frame, outcome) = answer.clone().into_parts();
    assert_eq!(answer.frame(), frame.as_deref());
    assert_eq!(answer.outcome(), &outcome);
    assert!(frame.is_some() && outcome.is_ok());
}
