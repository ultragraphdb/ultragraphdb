/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, lex/lexer.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use core::fmt::{Display, Formatter};

use ultragraphdb_common_utils::go_strconv;

use super::Item;
use crate::types::item_type::ITEM_EOF;

impl Display for Item<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        if self.typ == ITEM_EOF {
            return f.write_str("EOF");
        }
        write!(
            f,
            "lex.Item [{}] {} at {}:{}",
            self.typ,
            go_strconv::quote(&*self.val),
            self.line,
            self.column
        )
    }
}
