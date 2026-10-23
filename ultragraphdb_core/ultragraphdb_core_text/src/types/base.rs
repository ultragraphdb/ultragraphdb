/*
 * Copyright 2013 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, language/language.go (type Base). Changed for ultragraphdb: translated to
 * Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use core::fmt::{Display, Formatter};

use crate::types::lang_id::LangId;

/// An ISO 639 base language, displayed as its BCP 47 subtag (`und` when unspecified).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Base(LangId);

impl Base {
    pub(crate) fn new(id: LangId) -> Base {
        Base(id)
    }

    pub(crate) fn id(self) -> LangId {
        self.0
    }
}

impl Display for Base {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0.as_string())
    }
}
