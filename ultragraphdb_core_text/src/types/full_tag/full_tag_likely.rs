/*
 * Copyright 2013 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/language/match.go (addTags) and internal/language/language.go
 * (Tag.Parent). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use super::FullTag;
use crate::errors::language_error::{LanguageError, LanguageErrorEnum};
use crate::tables::language_tables::{
    LANG_EN, LANG_NO_INDEX_OFFSET, LIKELY_LANG, LIKELY_LANG_LIST, LIKELY_REGION,
    LIKELY_REGION_GROUP, LIKELY_REGION_LIST, LIKELY_SCRIPT, N_REGION_GROUPS, PARENTS,
    REGION_INCLUSION,
};
use crate::types::lang_id::LangId;
use crate::types::region_id::RegionId;
use crate::types::script_id::ScriptId;

const IS_LIST: u8 = 1;
const SCRIPT_IN_FROM: u8 = 2;

impl FullTag {
    fn set_undefined_lang(&mut self, id: LangId) {
        if self.lang_id.0 == 0 {
            self.lang_id = id;
        }
    }

    fn set_undefined_script(&mut self, id: ScriptId) {
        if self.script_id.0 == 0 {
            self.script_id = id;
        }
    }

    fn set_undefined_region(&mut self, id: RegionId) {
        if self.region_id.0 == 0 || self.region_id.contains(id) {
            self.region_id = id;
        }
    }

    /// Specializes a group region to the region of the group's most likely tag; reports whether the region was
    /// a group.
    fn specialize_region(&mut self) -> bool {
        let i = REGION_INCLUSION[usize::from(self.region_id.0)];
        if i < N_REGION_GROUPS {
            let x = LIKELY_REGION_GROUP[usize::from(i)];
            if LangId(x.lang) == self.lang_id && ScriptId(x.script) == self.script_id {
                self.region_id = RegionId(x.region);
            }
            return true;
        }
        false
    }

    /// `Maximize`, Go's `addTags`: the tag with missing subtags filled in. On an error Go returns the tag as it
    /// stands with the error; so does this port.
    pub(crate) fn maximize(&self) -> (FullTag, Option<LanguageError>) {
        let mut t = self.clone();
        // We leave private use identifiers alone.
        if t.is_private_use() {
            return (t, None);
        }
        if t.script_id.0 != 0 && t.region_id.0 != 0 {
            if t.lang_id.0 != 0 {
                // already fully specified
                t.specialize_region();
                return (t, None);
            }
            // Search matches for und-script-region. For these cases region will never be a group.
            let x = LIKELY_REGION[usize::from(t.region_id.0)];
            let list = if x.flags & IS_LIST != 0 {
                &LIKELY_REGION_LIST[usize::from(x.lang)..usize::from(x.lang + x.script)]
            } else {
                &LIKELY_REGION[usize::from(t.region_id.0)..usize::from(t.region_id.0) + 1]
            };
            for x in list {
                // Deviating from the spec. See match_test.go for details.
                if ScriptId(x.script) == t.script_id {
                    t.set_undefined_lang(LangId(x.lang));
                    return (t, None);
                }
            }
        }
        if t.lang_id.0 != 0 {
            // Search matches for lang-script and lang-region, where lang != und.
            if t.lang_id.0 < LANG_NO_INDEX_OFFSET {
                let x = LIKELY_LANG[usize::from(t.lang_id.0)];
                if x.flags & IS_LIST != 0 {
                    let list =
                        &LIKELY_LANG_LIST[usize::from(x.region)..usize::from(x.region + x.script)];
                    if t.script_id.0 != 0 {
                        for x in list {
                            if ScriptId(x.script) == t.script_id && x.flags & SCRIPT_IN_FROM != 0 {
                                t.set_undefined_region(RegionId(x.region));
                                return (t, None);
                            }
                        }
                    } else if t.region_id.0 != 0 {
                        let mut count = 0;
                        let mut good_script = true;
                        let mut tt = t.clone();
                        for x in list {
                            // Visit all entries for which the script was not defined, including the ones where the
                            // region was not defined, for proper disambiguation within regions.
                            if x.flags & SCRIPT_IN_FROM == 0
                                && t.region_id.contains(RegionId(x.region))
                            {
                                tt.region_id = RegionId(x.region);
                                tt.set_undefined_script(ScriptId(x.script));
                                good_script = good_script && tt.script_id == ScriptId(x.script);
                                count += 1;
                            }
                        }
                        if count == 1 {
                            return (tt, None);
                        }
                        // Even if we fail to find a unique region, we might have an unambiguous script.
                        if good_script {
                            t.script_id = tt.script_id;
                        }
                    }
                }
            }
        } else {
            // Search matches for und-script.
            if t.script_id.0 != 0 {
                let x = LIKELY_SCRIPT[usize::from(t.script_id.0)];
                if x.region != 0 {
                    t.set_undefined_region(RegionId(x.region));
                    t.set_undefined_lang(LangId(x.lang));
                    return (t, None);
                }
            }
            // Search matches for und-region. If und-script-region exists, it would have been found earlier.
            if t.region_id.0 != 0 {
                let i = REGION_INCLUSION[usize::from(t.region_id.0)];
                if i < N_REGION_GROUPS {
                    let x = LIKELY_REGION_GROUP[usize::from(i)];
                    if x.region != 0 {
                        t.set_undefined_lang(LangId(x.lang));
                        t.set_undefined_script(ScriptId(x.script));
                        t.region_id = RegionId(x.region);
                    }
                } else {
                    let mut x = LIKELY_REGION[usize::from(t.region_id.0)];
                    if x.flags & IS_LIST != 0 {
                        x = LIKELY_REGION_LIST[usize::from(x.lang)];
                    }
                    if x.script != 0 && x.flags != SCRIPT_IN_FROM {
                        t.set_undefined_lang(LangId(x.lang));
                        t.set_undefined_script(ScriptId(x.script));
                        return (t, None);
                    }
                }
            }
        }

        // Search matches for lang.
        if t.lang_id.0 < LANG_NO_INDEX_OFFSET {
            let mut x = LIKELY_LANG[usize::from(t.lang_id.0)];
            if x.flags & IS_LIST != 0 {
                x = LIKELY_LANG_LIST[usize::from(x.region)];
            }
            if x.region != 0 {
                t.set_undefined_script(ScriptId(x.script));
                t.set_undefined_region(RegionId(x.region));
            }
            t.specialize_region();
            if t.lang_id.0 == 0 {
                t.lang_id = LangId(LANG_EN); // default language
            }
            return (t, None);
        }
        (
            t,
            Some(LanguageError::new(LanguageErrorEnum::MissingLikelyTagsData)),
        )
    }

    /// The CLDR parent: a less specific tag that is mutually intelligible, or `und`.
    pub(crate) fn parent(&self) -> FullTag {
        let base = |lang: LangId| {
            FullTag::from_ids(lang, ScriptId(0), RegionId(0))
                .maximize()
                .0
        };
        if !self.str.is_empty() {
            // Strip the variants and extensions.
            let (b, s, r) = self.raw();
            let t = FullTag::from_ids(b, s, r);
            if t.region_id.0 == 0
                && t.script_id.0 != 0
                && t.lang_id.0 != 0
                && base(t.lang_id).script_id == t.script_id
            {
                return FullTag::from_ids(t.lang_id, ScriptId(0), RegionId(0));
            }
            return t;
        }
        if self.lang_id.0 != 0 {
            if self.region_id.0 != 0 {
                let mut max_script = self.script_id;
                if max_script.0 == 0 {
                    max_script = self.maximize().0.script_id;
                }
                for p in PARENTS.iter() {
                    if LangId(p.lang) == self.lang_id
                        && ScriptId(p.max_script) == max_script
                        && p.from_region.iter().any(|&r| RegionId(r) == self.region_id)
                    {
                        return FullTag::from_ids(
                            self.lang_id,
                            ScriptId(p.script),
                            RegionId(p.to_region),
                        );
                    }
                }
                // Strip the script if it is the default one.
                if base(self.lang_id).script_id != max_script {
                    return FullTag::from_ids(self.lang_id, max_script, RegionId(0));
                }
                return FullTag::from_ids(self.lang_id, ScriptId(0), RegionId(0));
            } else if self.script_id.0 != 0 {
                // The parent for a base-script pair with a non-default script is "und" instead of the base
                // language.
                if base(self.lang_id).script_id != self.script_id {
                    return FullTag::default();
                }
                return FullTag::from_ids(self.lang_id, ScriptId(0), RegionId(0));
            }
        }
        FullTag::default()
    }
}
