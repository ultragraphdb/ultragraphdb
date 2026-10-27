/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod operation_view_body_view;

use crate::types::string_list::StringList;

/// The body of a received Operation frame, read in place; see [`crate::Operation`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationView<'a> {
    kind: u8,
    delta: i64,
    properties: u32,
    links: u32,
    args: Vec<u32>,
    strings: StringList<'a>,
}

impl<'a> OperationView<'a> {
    pub fn kind(&self) -> u8 {
        self.kind
    }

    pub fn delta(&self) -> i64 {
        self.delta
    }

    pub fn properties(&self) -> u32 {
        self.properties
    }

    pub fn links(&self) -> u32 {
        self.links
    }

    pub fn strings(&self) -> &StringList<'a> {
        &self.strings
    }
}
