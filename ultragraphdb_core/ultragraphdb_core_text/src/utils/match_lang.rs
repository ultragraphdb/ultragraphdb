/*
 * Copyright 2015 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/colltab/colltab.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::confidence::Confidence;
use crate::types::tag::Tag;
use crate::utils::canonicalize::CanonType;
use crate::utils::compose::{Part, compose_raw};
use crate::utils::tag_index::partition_point;

/// The index of `t` in `tags`, matched as collation and search match languages. `tags[0]` must be `und`, the
/// other tags sorted by base language.
///
/// Unlike `language.Matcher`, the (inferred) base language must match exactly, so "gsw" does not match "de", and
/// a parent may have a different script: the parent of zh-Hant is zh here, not und.
pub(crate) fn match_lang(t: &Tag, tags: &[Tag]) -> usize {
    // Canonicalize the values, including collapsing macro languages.
    let mut t = t.canonicalize_with(CanonType::ALL);

    let (base, conf) = t.base();
    // Estimate the base language, but only use high-confidence values.
    if conf < Confidence::High {
        // The root locale supports "search" and "standard". We assume that any implementation will only use one
        // of both.
        return 0;
    }
    let base_id = base.id();

    // Maximize base and script and normalize the tag.
    let (_, s, r) = t.raw();
    if r.0 != 0 {
        let p = compose_raw(&[Part::Base(base_id), Part::Script(s), Part::Region(r)]);
        // Taking the parent forces the script to be maximized.
        let p = p.parent();
        // Add back region and extensions.
        t = compose_raw(&[
            Part::Tag(&p),
            Part::Region(r),
            Part::Extensions(t.extensions()),
        ]);
    } else {
        // Set the maximized base language.
        t = compose_raw(&[
            Part::Base(base_id),
            Part::Script(s),
            Part::Extensions(t.extensions()),
        ]);
    }

    // Find start index of the language tag.
    let base_str = base.to_string();
    let start = 1 + partition_point(tags.len() - 1, |i| {
        tags[i + 1].raw().0.as_string() < base_str
    });
    if start < tags.len() && tags[start].raw().0 != base_id {
        return 0;
    }

    // Besides the base language, script and region, only the collation type and the custom variant defined in
    // the 'u' extension distinguish a locale. Strip all variants and extensions and add back the custom variant.
    let (b, s, r) = t.raw();
    let tdef = compose_raw(&[Part::Base(b), Part::Script(s), Part::Region(r)]);
    let (tdef, _) = tdef.set_type_for_key("va", &t.type_for_key("va"));

    // First search for a specialized collation type, if present.
    let co = t.type_for_key("co");
    let tries = if co.is_empty() {
        vec![tdef]
    } else {
        vec![tdef.set_type_for_key("co", &co).0, tdef]
    };

    for mut tx in tries {
        while tx != Tag::default() {
            for (i, t) in tags[start..].iter().enumerate() {
                if t.raw().0 != base_id {
                    break;
                }
                if tx == *t {
                    return start + i;
                }
            }
            tx = parent(&tx);
        }
    }
    0
}

/// The structural parent: unlike the CLDR parent, inheritance may change the script, so parent(zh-Hant) is zh.
fn parent(t: &Tag) -> Tag {
    if !t.type_for_key("va").is_empty() {
        return t.set_type_for_key("va", "").0;
    }
    let (b, s, r) = t.raw();
    if r.0 != 0 {
        compose_raw(&[
            Part::Base(b),
            Part::Script(s),
            Part::Extensions(t.extensions()),
        ])
    } else if s.0 != 0 {
        compose_raw(&[Part::Base(b), Part::Extensions(t.extensions())])
    } else if b.0 != 0 {
        compose_raw(&[Part::Extensions(t.extensions())])
    } else {
        Tag::default()
    }
}
