/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use crate::types::defect::Defect;

/// The whole set of one target's files: each role's or instance's name, with the label its defects name it by and its bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TargetFiles {
    name: String,
    files: BTreeMap<String, (String, Vec<u8>)>,
}

impl TargetFiles {
    /// The target `name`, holding no file yet.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            files: BTreeMap::new(),
        }
    }

    /// Holds `bytes` as the file of `key`, named `label` in its defects.
    pub fn insert(&mut self, key: impl Into<String>, label: impl Into<String>, bytes: Vec<u8>) {
        let _previous = self.files.insert(key.into(), (label.into(), bytes));
    }

    /// The target's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Each file: its key, its label and its bytes.
    pub fn files(&self) -> impl Iterator<Item = (&str, &str, &[u8])> {
        self.files
            .iter()
            .map(|(key, (label, bytes))| (key.as_str(), label.as_str(), bytes.as_slice()))
    }

    /// The bytes of `key`'s file.
    #[must_use]
    pub fn bytes(&self, key: &str) -> Option<&[u8]> {
        self.files.get(key).map(|(_, bytes)| bytes.as_slice())
    }

    /// The label of `key`'s file.
    #[must_use]
    pub fn label(&self, key: &str) -> Option<&str> {
        self.files.get(key).map(|(label, _)| label.as_str())
    }

    /// Whether `other` holds the same keys with the same bytes, whatever each is labelled.
    #[must_use]
    pub fn same_files(&self, other: &Self) -> bool {
        self.files()
            .map(|(key, _, bytes)| (key, bytes))
            .eq(other.files().map(|(key, _, bytes)| (key, bytes)))
    }

    /// The files as they travel and are stored: for each, its name, a space, its length in decimal, a line break, its
    /// bytes and a line break.
    #[must_use]
    pub fn to_bundle(&self) -> Vec<u8> {
        let mut bundle = Vec::new();
        for (key, (_, bytes)) in &self.files {
            bundle.extend_from_slice(format!("{key} {}\n", bytes.len()).as_bytes());
            bundle.extend_from_slice(bytes);
            bundle.push(b'\n');
        }
        bundle
    }

    /// The target `name`'s files from `bundle`, each named by its key.
    ///
    /// # Errors
    ///
    /// A [`Defect`] saying where the bundle stops being one.
    pub fn from_bundle(name: &str, bundle: &[u8]) -> Result<Self, Box<Defect>> {
        let required = "a revision's files are each a name, a space, a decimal length, a line break, the bytes and a line break";
        let wrong = |at: usize| {
            Box::new(
                Defect::new(format!("the upload is malformed at byte {at}"), required)
                    .of_target(name),
            )
        };
        let mut target = Self::new(name);
        let mut at = 0;
        while at < bundle.len() {
            let rest = bundle.get(at..).ok_or_else(|| wrong(at))?;
            let line_end = rest
                .iter()
                .position(|byte| *byte == b'\n')
                .ok_or_else(|| wrong(at))?;
            let line = rest
                .get(..line_end)
                .and_then(|line| std::str::from_utf8(line).ok())
                .ok_or_else(|| wrong(at))?;
            let (key, length) = line.split_once(' ').ok_or_else(|| wrong(at))?;
            let length: usize = length.parse().map_err(|_| wrong(at))?;
            let start = at + line_end + 1;
            let end = start.checked_add(length).ok_or_else(|| wrong(at))?;
            let bytes = bundle.get(start..end).ok_or_else(|| wrong(start))?;
            if bundle.get(end) != Some(&b'\n') || key.is_empty() || target.bytes(key).is_some() {
                return Err(wrong(end));
            }
            target.insert(key, key, bytes.to_vec());
            at = end + 1;
        }
        Ok(target)
    }
}
