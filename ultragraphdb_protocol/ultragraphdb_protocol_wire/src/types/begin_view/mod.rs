/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod begin_view_body_view;

use std::marker::PhantomData;

/// The body of a received Begin frame, read in place; see [`crate::Begin`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeginView<'a> {
    frame: PhantomData<&'a [u8]>,
    namespace: u64,
    read_version: u64,
    read_only: bool,
}

impl<'a> BeginView<'a> {
    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    pub fn read_only(&self) -> bool {
        self.read_only
    }
}
