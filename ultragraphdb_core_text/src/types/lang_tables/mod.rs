/*
 * Copyright 2013 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/language/tables.go (the table row types). Changed for ultragraphdb:
 * translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! The row types of the language tables.

mod from_to;
mod likely_lang_region;
mod likely_lang_script;
mod likely_script_region;
mod likely_tag;
mod parent_rel;

pub(crate) use from_to::FromTo;
pub(crate) use likely_lang_region::LikelyLangRegion;
pub(crate) use likely_lang_script::LikelyLangScript;
pub(crate) use likely_script_region::LikelyScriptRegion;
pub(crate) use likely_tag::LikelyTag;
pub(crate) use parent_rel::ParentRel;
