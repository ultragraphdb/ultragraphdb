/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod abort_view_body_view;

use std::marker::PhantomData;

/// The body of a received Abort frame, read in place; see [`crate::Abort`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbortView<'a> {
    frame: PhantomData<&'a [u8]>,
}
