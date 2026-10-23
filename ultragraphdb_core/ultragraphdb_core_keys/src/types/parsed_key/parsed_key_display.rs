/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, x/keys.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::fmt::{Display, Formatter, Result};

use ultragraphdb_common_utils::go_fmt;

use super::ParsedKey;

/// Dgraph's `ParsedKey.String`, which its parse errors print.
impl Display for ParsedKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if self.is_index() {
            write!(
                f,
                "UID: {}, Attr: {}, IsIndex: true, Term: {}",
                self.uid,
                self.attr,
                go_fmt::byte_slice(&self.term)
            )
        } else if self.is_count_or_count_rev() {
            write!(
                f,
                "UID: {}, Attr: {}, IsCount/Ref: true, Count: {}",
                self.uid, self.attr, self.count
            )
        } else {
            write!(
                f,
                "UID: {}, Attr: {}, Data key, prefix; {}, byte: {}",
                self.uid, self.attr, self.byte_prefix, self.byte_type
            )
        }
    }
}
