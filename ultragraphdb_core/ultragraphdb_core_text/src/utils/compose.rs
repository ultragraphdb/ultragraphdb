/*
 * Copyright 2013 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, language/parse.go (Compose and update). Changed for ultragraphdb:
 * translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::types::builder::Builder;
use crate::types::lang_id::LangId;
use crate::types::region_id::RegionId;
use crate::types::script_id::ScriptId;
use crate::types::tag::Tag;

/// A part `Compose` takes: the parts the collator's language matching passes.
pub(crate) enum Part<'a> {
    Tag(&'a Tag),
    Base(LangId),
    Script(ScriptId),
    Region(RegionId),
    /// A `[]Extension`, which replaces all extensions; `None` is Go's nil slice.
    Extensions(Option<Vec<String>>),
}

/// `language.Raw.Compose`: a tag from its parts, later parts overwriting earlier ones, without
/// canonicalization. A panic is recovered as `und`, as Go does.
pub(crate) fn compose_raw(parts: &[Part<'_>]) -> Tag {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let mut b = Builder::default();
        for part in parts {
            match part {
                Part::Tag(t) => b.set_tag(&t.tag()),
                Part::Base(l) => b.tag.lang_id = *l,
                Part::Script(s) => b.tag.script_id = *s,
                Part::Region(r) => b.tag.region_id = *r,
                Part::Extensions(exts) => {
                    b.clear_extensions();
                    for e in exts.iter().flatten() {
                        b.set_ext(e);
                    }
                }
            }
        }
        Tag::make(b.make())
    }));
    result.unwrap_or_default()
}
