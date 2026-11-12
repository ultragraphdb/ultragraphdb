/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod reference_body_builder;

use ultragraphdb_core_ids::{BucketGroup, SystemId};

use crate::types::reference_mode::ReferenceMode;
use crate::types::reference_registration::ReferenceRegistration;
use crate::types::reference_status::ReferenceStatus;
use crate::types::request_term::RequestTerm;
use crate::types::term_table::TermTable;

/// A partition registers its references at an entity's home, or gets or creates a predicate's or graph's ID there
/// (`PROTOCOL.md` §13.15); the home answers with every field as sent and each registration's reply fields filled.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `reply` | u8 boolean: 0 the request, 1 the home's reply |
/// | 1 | `mode` | u8: 0 get or create, 1 get-only, for participant checks |
/// | 8 | `tenant` | u64: the tenant of the batch's entities |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | registrations, 48 bytes each ([`ReferenceRegistration`]), at least one |
/// | 1 | the term table's records: the IRIs registrations name by their text, each an IRI entry |
/// | 2 | the term table's text |
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Reference {
    tenant: u64,
    reply: bool,
    mode: ReferenceMode,
    registrations: Vec<ReferenceRegistration>,
    terms: TermTable,
}

impl Reference {
    /// A request with no registrations yet.
    #[must_use]
    pub fn request() -> Self {
        Self::default()
    }

    /// The body of `registrations` over `terms`, the request or, with `reply`, the home's reply.
    #[must_use]
    pub fn new(reply: bool, registrations: Vec<ReferenceRegistration>, terms: TermTable) -> Self {
        Self {
            tenant: 0,
            reply,
            mode: ReferenceMode::GetOrCreate,
            registrations,
            terms,
        }
    }

    /// Appends the registration of the entity named by its IRI `iri` (or a blank node's skolem IRI) at the home
    /// `partition`, for `group`, with the `generation` last seen; its index among the registrations.
    pub fn register_iri(
        &mut self,
        partition: u32,
        group: Option<BucketGroup>,
        iri: &str,
        generation: u64,
    ) -> usize {
        let index = self.terms.iri(iri);
        self.push(ReferenceRegistration::request(
            partition,
            group,
            Some(index),
            None,
            generation,
        ))
    }

    /// Appends the registration of the entity named by its system ID `entity`; its index among the registrations.
    pub fn register_entity(
        &mut self,
        partition: u32,
        group: Option<BucketGroup>,
        entity: SystemId,
        generation: u64,
    ) -> usize {
        self.push(ReferenceRegistration::request(
            partition,
            group,
            None,
            Some(entity),
            generation,
        ))
    }

    /// Appends `registration`; its index.
    fn push(&mut self, registration: ReferenceRegistration) -> usize {
        let index = self.registrations.len();
        self.registrations.push(registration);
        index
    }

    /// The batch in `tenant`, whose dictionary the home resolves its IRIs in.
    #[must_use]
    pub fn in_tenant(mut self, tenant: u64) -> Self {
        self.tenant = tenant;
        self
    }

    /// The tenant of the batch's entities.
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The home's reply to this request: every registration as sent, until [`Reference::accept`] or
    /// [`Reference::refuse`] fills it.
    #[must_use]
    pub fn into_reply(self) -> Self {
        Self {
            reply: true,
            ..self
        }
    }

    /// Fills registration `index` as accepted, with the entity's ID, the member's `generation` and the latest
    /// `close` delivered to the home; false when there is no registration `index`.
    pub fn accept(&mut self, index: usize, entity: SystemId, generation: u64, close: u64) -> bool {
        self.registrations
            .get_mut(index)
            .map(|registration| *registration = registration.accepted(entity, generation, close))
            .is_some()
    }

    /// Fills registration `index` as refused with the error code `outcome`; false when there is no registration
    /// `index`.
    pub fn refuse(&mut self, index: usize, outcome: u16) -> bool {
        self.registrations
            .get_mut(index)
            .map(|registration| *registration = registration.refused(outcome))
            .is_some()
    }

    /// Whether this is the home's reply.
    #[must_use]
    pub fn is_reply(&self) -> bool {
        self.reply
    }

    /// The registrations, in order.
    #[must_use]
    pub fn registrations(&self) -> &[ReferenceRegistration] {
        &self.registrations
    }

    /// The term table of the IRIs the registrations name.
    #[must_use]
    pub fn terms(&self) -> &TermTable {
        &self.terms
    }

    /// The IRI registration `index` names the entity by; `None` when its system ID names it or past the last.
    #[must_use]
    pub fn iri(&self, index: usize) -> Option<&str> {
        match self.terms.get(self.registrations.get(index)?.iri()?)? {
            RequestTerm::Iri(iri) => Some(iri),
            RequestTerm::BlankNode(_)
            | RequestTerm::Literal { .. }
            | RequestTerm::LanguageLiteral { .. }
            | RequestTerm::Triple { .. } => None,
        }
    }

    /// The batch in `mode`: get or create, or get-only for participant checks.
    #[must_use]
    pub fn with_mode(self, mode: ReferenceMode) -> Self {
        Self { mode, ..self }
    }

    /// The batch's mode.
    #[must_use]
    pub fn mode(&self) -> ReferenceMode {
        self.mode
    }

    /// Registration `index` of a get-only batch accepted with `status`: the entity's ID for a live, deleted or
    /// retracted one, `None` for an absent one; for a live one naming a bucket group, the member's `generation` and
    /// the latest `close` delivered to the home, else 0. `false` past the last registration.
    pub fn answer_get_only(
        &mut self,
        index: usize,
        status: ReferenceStatus,
        entity: Option<SystemId>,
        generation: u64,
        close: u64,
    ) -> bool {
        self.registrations
            .get_mut(index)
            .map(|registration| {
                *registration = registration.answered(status, entity, generation, close);
            })
            .is_some()
    }
}
