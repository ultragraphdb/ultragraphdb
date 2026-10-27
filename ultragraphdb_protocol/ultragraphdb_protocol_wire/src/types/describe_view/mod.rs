/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod describe_view_body_view;

use crate::types::string_list::StringList;
use crate::utils::changes::CHANGE_RECORD_SIZE;

/// The body of a received Describe frame, read in place; see [`crate::Describe`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescribeView<'a> {
    namespace: u64,
    read_version: u64,
    strings: StringList<'a>,
    labels: Vec<u32>,
    relationship_types: Vec<u32>,
    property_keys: Vec<u32>,
    records: &'a [u8],
    args: Vec<u32>,
}

impl<'a> DescribeView<'a> {
    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    pub fn strings(&self) -> &StringList<'a> {
        &self.strings
    }

    /// The labels, in order.
    pub fn labels(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.labels
            .iter()
            .filter_map(|i| self.strings.get(*i as usize))
    }

    /// The relationship types, in order.
    pub fn relationship_types(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.relationship_types
            .iter()
            .filter_map(|i| self.strings.get(*i as usize))
    }

    /// The property keys, in order.
    pub fn property_keys(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.property_keys
            .iter()
            .filter_map(|i| self.strings.get(*i as usize))
    }

    /// The number of declarations.
    pub fn declarations(&self) -> usize {
        self.records.len() / CHANGE_RECORD_SIZE
    }
}
