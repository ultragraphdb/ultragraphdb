/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod register_view_body_view;

use std::marker::PhantomData;

/// The body of a received Register frame, read in place; see [`crate::Register`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisterView<'a> {
    frame: PhantomData<&'a [u8]>,
    session: u64,
}

impl<'a> RegisterView<'a> {
    pub fn session(&self) -> u64 {
        self.session
    }
}
