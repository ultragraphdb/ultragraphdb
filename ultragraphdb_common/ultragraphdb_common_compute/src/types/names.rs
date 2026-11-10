/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashMap;

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_terms::{Datatype, EncodedTerm, Hash128, TermView, Text};
use ultragraphdb_store_quads::{id_key, literal_key};

use crate::types::wanted_names::WantedNames;

/// The dictionary texts a read holds at most per partition before it reads them again.
const TEXTS_MAX: usize = 1 << 14;

/// The dictionary bytes a read holds at most per partition before it reads them again: a few of the largest Result frames'.
const TEXT_BYTES_MAX: usize = 4 << 20;

/// The dictionary's texts of one partition, as far as a read has read them: what names a row's system IDs and hashes.
#[derive(Debug)]
pub(crate) struct Names {
    tenant: u64,
    texts: HashMap<Vec<u8>, String>,
    bytes: usize,
}

impl Names {
    /// None yet, of `tenant`.
    pub(crate) fn new(tenant: u64) -> Self {
        Self {
            tenant,
            texts: HashMap::new(),
            bytes: 0,
        }
    }

    /// Lets go of every text once too many or too large are held. Called with no row waiting on a name it holds.
    pub(crate) fn shed(&mut self) {
        if self.texts.len() > TEXTS_MAX || self.bytes > TEXT_BYTES_MAX {
            self.texts.clear();
            self.bytes = 0;
        }
    }

    /// Takes the dictionary's `text` of `key`.
    pub(crate) fn hold(&mut self, key: Vec<u8>, text: String) {
        self.bytes = self
            .bytes
            .saturating_add(key.len())
            .saturating_add(text.len());
        let _none = self.texts.insert(key, text);
    }

    /// The text of `id`.
    pub(crate) fn of_id(&self, id: SystemId) -> Option<&str> {
        self.texts.get(&id_key(self.tenant, id)).map(String::as_str)
    }

    /// The text of `hash`.
    pub(crate) fn of_hash(&self, hash: Hash128) -> Option<&str> {
        self.texts
            .get(&literal_key(self.tenant, &hash))
            .map(String::as_str)
    }

    /// The dictionary key of `id`, into `keys` unless held.
    pub(crate) fn want_id(&self, id: SystemId, keys: &mut WantedNames) {
        self.want(id_key(self.tenant, id), keys);
    }

    /// The dictionary keys of what `term` names by ID or hash, into `keys` unless held. An encoding that does not read names
    /// nothing, and refuses when the row is built.
    pub(crate) fn want_term(&self, term: &EncodedTerm, keys: &mut WantedNames) {
        let Ok(view) = term.view() else {
            return;
        };
        match view {
            TermView::Id(id) => self.want_id(id, keys),
            TermView::String(text) => self.want_text(text, keys),
            TermView::LangString {
                lexical, language, ..
            } => {
                self.want_text(lexical, keys);
                self.want_text(language, keys);
            }
            TermView::Typed { datatype, lexical } => {
                if let Datatype::Id(id) = datatype {
                    self.want_id(id, keys);
                }
                self.want_text(lexical, keys);
            }
            TermView::Boolean(_)
            | TermView::Integer(_)
            | TermView::Decimal(_)
            | TermView::Double(_)
            | TermView::Float(_)
            | TermView::DateTime(_)
            | TermView::Time(_)
            | TermView::Date(_)
            | TermView::GYearMonth(_)
            | TermView::GYear(_)
            | TermView::GMonthDay(_)
            | TermView::GDay(_)
            | TermView::GMonth(_)
            | TermView::Duration(_)
            | TermView::YearMonthDuration(_)
            | TermView::DayTimeDuration(_)
            | TermView::Vector(_)
            | TermView::Bfloat16Vector(_)
            | TermView::Measure { .. }
            | TermView::Precision(_) => {}
        }
    }

    fn want_text(&self, text: Text<'_>, keys: &mut WantedNames) {
        match text {
            Text::Inline(_) => {}
            Text::Hashed(hash) => self.want(literal_key(self.tenant, &hash), keys),
        }
    }

    fn want(&self, key: Vec<u8>, keys: &mut WantedNames) {
        if !self.texts.contains_key(&key) {
            keys.want(key);
        }
    }
}
