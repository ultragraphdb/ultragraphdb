/*
 * Copyright 2013 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, language/language.go and language/parse.go (type Tag). Changed for
 * ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod tag_compact;
mod tag_display;

use crate::errors::language_error::LanguageError;
use crate::types::base::Base;
use crate::types::confidence::Confidence;
use crate::types::full_tag::FullTag;
use crate::types::lang_id::LangId;
use crate::types::region_id::RegionId;
use crate::types::script_id::ScriptId;
use crate::utils::canonicalize::{CanonType, canonicalize};
use crate::utils::parse;

/// A BCP 47 language tag, as `golang.org/x/text/language` holds it: a compact index for the common tags, the full
/// tag otherwise. The default value is `und`, and two tags are equal exactly when Go's are.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tag {
    language: u16,
    locale: u16,
    full: Option<FullTag>,
}

impl Tag {
    /// Parses a BCP 47 tag and canonicalizes it with Go's default canonicalization (deprecated and legacy
    /// subtags replaced), as `language.Parse` does.
    ///
    /// Go returns the part of the tag it could parse along with the error; Dgraph never uses it, so the error
    /// alone is returned here.
    pub fn parse(s: &str) -> Result<Tag, LanguageError> {
        Tag::parse_with(CanonType::DEFAULT, s)
    }

    /// `CanonType.Parse`: parses and canonicalizes with `c`. Go recovers a panic in its parser as a syntax error;
    /// the port's parser, canonicalization and compaction refuse with errors and do not panic.
    pub(crate) fn parse_with(c: CanonType, s: &str) -> Result<Tag, LanguageError> {
        match parse::parse(s) {
            Err((_, err)) => Err(err),
            Ok(tt) => {
                let (mut tt, changed) = canonicalize(c, tt);
                if changed {
                    tt.remake_string();
                }
                Ok(Tag::make(tt))
            }
        }
    }

    /// The internal form of the tag.
    pub(crate) fn tag(&self) -> FullTag {
        self.to_full()
    }

    /// The base language, script and region as given, without inferring missing ones.
    pub(crate) fn raw(&self) -> (LangId, ScriptId, RegionId) {
        self.tag().raw()
    }

    /// The base language of the tag. If it is unspecified, it is inferred with a variant of CLDR's Add Likely
    /// Subtags algorithm, and the confidence says how certain the inference is.
    pub fn base(&self) -> (Base, Confidence) {
        let tt = self.tag();
        if tt.lang_id.0 != 0 {
            return (Base::new(tt.lang_id), Confidence::Exact);
        }
        let c = if tt.script_id.0 == 0 && !tt.region_id.is_country() {
            Confidence::Low
        } else {
            Confidence::High
        };
        if let (tag, None) = tt.maximize()
            && tag.lang_id.0 != 0
        {
            return (Base::new(tag.lang_id), c);
        }
        (Base::new(LangId(0)), Confidence::No)
    }

    /// The canonical string form of the tag.
    pub(crate) fn as_string(&self) -> String {
        self.tag().as_string()
    }

    /// The CLDR parent of the tag.
    pub(crate) fn parent(&self) -> Tag {
        self.compact_parent()
    }

    /// All extensions of the tag, or `None` when it cannot have any.
    pub(crate) fn extensions(&self) -> Option<Vec<String>> {
        if !self.may_have_extensions() {
            return None;
        }
        Some(self.tag().extensions())
    }

    /// The first type of the `-u` extension key `key`, or the empty string.
    pub(crate) fn type_for_key(&self, key: &str) -> String {
        if !self.may_have_extensions() && key != "rg" && key != "va" {
            return String::new();
        }
        self.tag().type_for_key(key)
    }

    /// The tag with the `-u` extension key `key` set to `value`; an empty value removes the key. On an error the
    /// tag is returned unchanged with it.
    pub(crate) fn set_type_for_key(&self, key: &str, value: &str) -> (Tag, Option<LanguageError>) {
        match self.tag().set_type_for_key(key, value) {
            Ok(tt) => (Tag::make(tt), None),
            Err((tt, err)) => (Tag::make(tt), Some(err)),
        }
    }

    /// `CanonType.Canonicalize`: the tag canonicalized with `c`.
    pub(crate) fn canonicalize_with(&self, c: CanonType) -> Tag {
        // First try fast path.
        if self.is_compact() {
            let (_, changed) = canonicalize(c, self.to_full());
            if !changed {
                return self.clone();
            }
        }
        let (mut tag, changed) = canonicalize(c, self.tag());
        if changed {
            tag.remake_string();
            return Tag::make(tag);
        }
        self.clone()
    }
}
