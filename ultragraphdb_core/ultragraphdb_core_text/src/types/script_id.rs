/*
 * Copyright 2013 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/language/lookup.go (type Script). Changed for ultragraphdb:
 * translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::tables::language_tables::SCRIPT;
use crate::utils::tag_index::elem;

/// An ISO 15924 script identifier: an index into the script table. Zero is unspecified.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ScriptId(pub u16);

impl ScriptId {
    /// The script code in title case, `Zzzz` for an unspecified script.
    pub(crate) fn as_string(self) -> String {
        if self.0 == 0 {
            return "Zzzz".to_string();
        }
        String::from_utf8_lossy(elem(SCRIPT, usize::from(self.0))).into_owned()
    }
}
