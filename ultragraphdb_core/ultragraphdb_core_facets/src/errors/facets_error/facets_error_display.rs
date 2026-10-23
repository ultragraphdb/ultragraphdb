/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/facets/utils.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::fmt;

use ultragraphdb_common_utils::go_strconv;

use crate::errors::facets_error::{FacetsError, FacetsErrorEnum};

impl fmt::Display for FacetsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            FacetsErrorEnum::RepeatedKey(key) => {
                write!(f, "Repeated keys are not allowed in facets. But got {key}")
            }
            // errors.Wrapf(strconv.ErrSyntax, "could not unquote %q:", val)
            FacetsErrorEnum::Unquote(val) => {
                write!(
                    f,
                    "could not unquote {}:: invalid syntax",
                    go_strconv::quote(val)
                )
            }
            FacetsErrorEnum::Number(msg) => f.write_str(msg),
            FacetsErrorEnum::NaN => f.write_str("Got invalid value: NaN"),
            FacetsErrorEnum::Unparsable(val) => {
                write!(f, "Could not parse the facet value : [{val}]")
            }
            FacetsErrorEnum::UnrecognizedType(t) => write!(f, "Unrecognized facet type: {t}"),
            FacetsErrorEnum::Types(e) => e.fmt(f),
            FacetsErrorEnum::Tok(e) => e.fmt(f),
        }
    }
}
