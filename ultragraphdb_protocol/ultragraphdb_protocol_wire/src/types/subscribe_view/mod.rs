/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod subscribe_view_body_view;

use std::marker::PhantomData;

/// The body of a received Subscribe frame, read in place; see [`crate::Subscribe`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubscribeView<'a> {
    frame: PhantomData<&'a [u8]>,
    from: u64,
}

impl<'a> SubscribeView<'a> {
    /// The last epoch whose close the subscriber holds; 0 for none.
    #[must_use]
    pub fn from(&self) -> u64 {
        self.from
    }
}
