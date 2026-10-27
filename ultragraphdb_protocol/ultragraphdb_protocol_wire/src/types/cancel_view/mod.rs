/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod cancel_view_body_view;

use std::marker::PhantomData;

/// The body of a received Cancel frame, read in place; see [`crate::Cancel`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CancelView<'a> {
    frame: PhantomData<&'a [u8]>,
}
