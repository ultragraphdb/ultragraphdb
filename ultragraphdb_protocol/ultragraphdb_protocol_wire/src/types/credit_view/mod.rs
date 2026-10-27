/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod credit_view_body_view;

use std::marker::PhantomData;

/// The body of a received Credit frame, read in place; see [`crate::Credit`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreditView<'a> {
    frame: PhantomData<&'a [u8]>,
    frames: u32,
}

impl<'a> CreditView<'a> {
    pub fn frames(&self) -> u32 {
        self.frames
    }
}
