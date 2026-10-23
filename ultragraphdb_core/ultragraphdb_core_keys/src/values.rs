/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, x/values.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::value_type_info::ValueTypeInfo;

/// Decides the value type of a DirectedEdge, Posting or N-Quad. Every combination has a type, so
/// [`ValueTypeInfo::Unknown`] is never returned, as in Dgraph.
pub fn value_type(has_value: bool, has_lang: bool, has_special_id: bool) -> ValueTypeInfo {
    match (has_value, has_lang, has_special_id) {
        (true, true, _) => ValueTypeInfo::Multi,
        (true, false, _) => ValueTypeInfo::Plain,
        (false, _, true) => ValueTypeInfo::Empty,
        (false, _, false) => ValueTypeInfo::Uid,
    }
}
