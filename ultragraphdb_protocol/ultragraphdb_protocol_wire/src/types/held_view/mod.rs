/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod held_view_body_view;

use std::marker::PhantomData;

/// The body of a received Held frame, read in place; see [`crate::Held`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeldView<'a> {
    frame: PhantomData<&'a [u8]>,
}
