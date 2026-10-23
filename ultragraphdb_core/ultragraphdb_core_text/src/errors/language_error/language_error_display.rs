/*
 * Copyright 2013 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/language/parse.go, internal/language/language.go and
 * internal/language/match.go (the error texts). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use core::fmt::{Display, Formatter};

use ultragraphdb_common_utils::go_strconv;

use super::{LanguageError, LanguageErrorEnum};

impl Display for LanguageError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match &self.0 {
            LanguageErrorEnum::Syntax => write!(f, "language: tag is not well-formed"),
            LanguageErrorEnum::DuplicateKey => {
                write!(f, "language: different values for same key in -u extension")
            }
            LanguageErrorEnum::Value(tag) => {
                write!(
                    f,
                    "language: subtag {} is well-formed but unknown",
                    go_strconv::quote(tag)
                )
            }
            LanguageErrorEnum::PrivateUse => write!(f, "cannot set a key on a private use tag"),
            LanguageErrorEnum::InvalidArguments => write!(f, "invalid key or type"),
            LanguageErrorEnum::MissingLikelyTagsData => write!(f, "missing likely tags data"),
        }
    }
}
