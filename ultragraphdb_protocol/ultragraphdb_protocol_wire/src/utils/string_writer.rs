/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A list of strings as two sections: the u32 end offset of each string in the text, then the text, UTF-8.

use std::collections::HashMap;

use crate::utils::sections::{as_u32, u32_bytes};

/// A list of strings being written, each pushed or interned once.
#[derive(Debug, Clone, Default)]
pub(crate) struct StringWriter {
    ends: Vec<u32>,
    text: Vec<u8>,
    interned: HashMap<String, u32>,
}

impl StringWriter {
    /// The index of `s`, appended.
    pub(crate) fn push(&mut self, s: &str) -> u32 {
        self.text.extend_from_slice(s.as_bytes());
        self.ends.push(as_u32(self.text.len()));
        as_u32(self.ends.len() - 1)
    }

    /// The index of `s`, appended the first time only.
    pub(crate) fn intern(&mut self, s: &str) -> u32 {
        if let Some(index) = self.interned.get(s) {
            return *index;
        }
        let index = self.push(s);
        self.interned.insert(s.to_string(), index);
        index
    }

    /// The two sections: the ends, then the text.
    pub(crate) fn sections(&self) -> (Vec<u8>, Vec<u8>) {
        (u32_bytes(&self.ends), self.text.clone())
    }
}
