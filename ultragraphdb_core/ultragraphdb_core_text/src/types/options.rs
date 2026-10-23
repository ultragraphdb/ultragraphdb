/*
 * Copyright 2014 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, collate/option.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::elem::{IDENTITY, NUM_LEVELS, QUATERNARY, SECONDARY, TERTIARY};
use crate::types::tag::Tag;
use crate::types::weighter::Weighter;

/// The handling of variables: runes with a primary weight up to the variable top.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AlternateHandling {
    /// Variables are not handled specially.
    NonIgnorable,
    /// Variables and all subsequent primary ignorables are ignorable at all levels.
    Blanked,
    /// Variables are ignorable at levels one to three and add a fourth level from the ignored ones.
    Shifted,
    /// A variant of shifted used to emulate POSIX.
    ShiftTrimmed,
}

/// The settings of a collator.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Options {
    /// The levels to ignore.
    pub ignore: [bool; NUM_LEVELS],
    /// An additional level of case matching between the secondary and tertiary levels.
    pub case_level: bool,
    /// Sort the secondary level backwards, as French does with accents.
    pub backwards: bool,
    /// Sort sequences of decimal digits at the primary level by their numeric value.
    pub numeric: bool,
    pub alternate: AlternateHandling,
    pub t: Weighter,
}

impl Options {
    /// `newCollator`: the default options over a weighter.
    pub(crate) fn new(t: Weighter) -> Options {
        let mut ignore = [false; NUM_LEVELS];
        ignore[QUATERNARY] = true;
        ignore[IDENTITY] = true;
        Options {
            ignore,
            case_level: false,
            backwards: false,
            numeric: false,
            alternate: AlternateHandling::NonIgnorable,
            t,
        }
    }

    /// Sets the options of the BCP 47 `-u` extension of the tag.
    pub(crate) fn set_from_tag(&mut self, t: &Tag) {
        self.case_level = ldml_bool(t, self.case_level, "kc");
        self.backwards = ldml_bool(t, self.backwards, "kb");
        self.numeric = ldml_bool(t, self.numeric, "kn");

        // Extract settings from the BCP47 u extension.
        match t.type_for_key("ks").as_str() {
            // strength
            "level1" => {
                self.ignore[SECONDARY] = true;
                self.ignore[TERTIARY] = true;
            }
            "level2" => self.ignore[TERTIARY] = true,
            "level4" => self.ignore[QUATERNARY] = false,
            "identic" => {
                self.ignore[QUATERNARY] = false;
                self.ignore[IDENTITY] = false;
            }
            _ => {} // "level3" and "", the default
        }

        match t.type_for_key("ka").as_str() {
            "shifted" => self.alternate = AlternateHandling::Shifted,
            // Not official BCP 47; they give access to otherwise hidden functionality.
            "blanked" => self.alternate = AlternateHandling::Blanked,
            "posix" => self.alternate = AlternateHandling::ShiftTrimmed,
            _ => {}
        }
    }
}

fn ldml_bool(t: &Tag, old: bool, key: &str) -> bool {
    match t.type_for_key(key).as_str() {
        "true" => true,
        "false" => false,
        _ => old,
    }
}
