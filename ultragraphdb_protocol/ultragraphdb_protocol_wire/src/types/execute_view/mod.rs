/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod execute_view_body_view;

use crate::types::string_list::StringList;

/// The body of a received Execute frame, read in place; see [`crate::Execute`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecuteView<'a> {
    statement: u64,
    limit: u32,
    parameters: StringList<'a>,
    resume: &'a [u8],
}

impl<'a> ExecuteView<'a> {
    pub fn statement(&self) -> u64 {
        self.statement
    }

    pub fn limit(&self) -> u32 {
        self.limit
    }

    pub fn parameters(&self) -> &StringList<'a> {
        &self.parameters
    }

    pub fn resume(&self) -> &'a [u8] {
        self.resume
    }
}
