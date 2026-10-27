/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, BodyView, Register, RegisterView};

use crate::utils::golden_bodies::{frame_of, golden};

#[test]
fn test_a_request_carries_session_zero() {
    let bytes = Register::new(0).frame(golden("register").header()).unwrap();
    assert_eq!(
        RegisterView::from_frame(&frame_of(&bytes))
            .unwrap()
            .session(),
        0
    );
    let max = Register::new(u64::MAX)
        .frame(golden("register").header())
        .unwrap();
    assert_eq!(
        RegisterView::from_frame(&frame_of(&max)).unwrap().session(),
        u64::MAX
    );
}
