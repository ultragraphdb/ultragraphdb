/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod declare_body_builder;

use crate::types::declared::Declared;
use crate::types::drop_kind::DropKind;

/// Compute asks the system group for a catalog version (`PROTOCOL.md` §13.27): of `kind` 1 the version a
/// transaction's declarations make, built on the version current when the transaction began; of `kind` 2 the version
/// a committed Drop calls for. The reply holds every field as sent with `version` and `commit_version` filled.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `kind` | u8: 1 a transaction's declarations, 2 a Drop's removal |
/// | 1 | `drop` | u8: for `kind` 2 the Drop's kind, 1 to 4; 0 for `kind` 1 |
/// | 8 | `tenant` | u64 |
/// | 16 | `built_on` | u64: for `kind` 1 the tenant's catalog version when the transaction began; 0 for `kind` 2 |
/// | 24 | `principal` | u64: the client's principal |
/// | 32 | `version` | u64: 0 in a request; in a reply the catalog version committed as pending, 0 for a `kind` 2 that changed nothing |
/// | 40 | `commit_version` | u64: 0 in a request and for `kind` 2; in a `kind` 1 reply the first version of the effective epoch, its low 22 bits zero |
///
/// The sections are a Write's ([`crate::Write`]): for `kind` 1 section 0 holds the declarations, records of kinds 8 to
/// 10, and the names they name; for `kind` 2 it is empty and the term table holds the dropped predicate's or graph's
/// IRI, for a Drop of kind 1 or 2 alone.
///
/// [`crate::BodyBuilder::frame`] refuses what [`crate::DeclareView`] refuses: no declaration, a Drop of kind 1 or 2
/// without its target or one of kind 3 or 4 with one, a `commit_version` whose low 22 bits are not zero or one given
/// for a Drop, and a `kind` 1 reply whose `version` or `commit_version` is 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declare {
    tenant: u64,
    built_on: u64,
    principal: u64,
    version: u64,
    commit_version: u64,
    asked: Asked,
}

/// What a Declare asks a version for.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Asked {
    /// A transaction's declarations, in the order its Writes held them.
    Declarations(Vec<Declared>),
    /// A Drop's removal: its kind and, for a predicate or a graph, the IRI dropped.
    Drop(DropKind, Option<String>),
}

impl Declare {
    /// The request of `kind` 1: `principal`'s `declarations` in `tenant`, built on its catalog version `built_on`, 0
    /// for a tenant holding none.
    #[must_use]
    pub fn declarations(
        tenant: u64,
        built_on: u64,
        principal: u64,
        declarations: Vec<Declared>,
    ) -> Self {
        Self {
            tenant,
            built_on,
            principal,
            version: 0,
            commit_version: 0,
            asked: Asked::Declarations(declarations),
        }
    }

    /// The request of `kind` 2: `principal`'s Drop of kind `drop` in `tenant`, `target` the IRI of the predicate or
    /// graph dropped and `None` for the other kinds.
    #[must_use]
    pub fn drop(tenant: u64, principal: u64, drop: DropKind, target: Option<String>) -> Self {
        Self {
            tenant,
            built_on: 0,
            principal,
            version: 0,
            commit_version: 0,
            asked: Asked::Drop(drop, target),
        }
    }

    /// The reply: the request with the catalog `version` committed as pending and, for `kind` 1, the
    /// `commit_version`, the first version of the effective epoch.
    #[must_use]
    pub fn with_version(self, version: u64, commit_version: u64) -> Self {
        Self {
            version,
            commit_version,
            ..self
        }
    }

    /// The tenant.
    #[must_use]
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The catalog version the declarations are built on; 0 for a Drop.
    #[must_use]
    pub fn built_on(&self) -> u64 {
        self.built_on
    }

    /// The client's principal.
    #[must_use]
    pub fn principal(&self) -> u64 {
        self.principal
    }

    /// The catalog version committed as pending; 0 in a request.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The first version of the effective epoch; 0 in a request and for a Drop.
    #[must_use]
    pub fn commit_version(&self) -> u64 {
        self.commit_version
    }

    /// The Drop's kind for `kind` 2; `None` for `kind` 1.
    #[must_use]
    pub fn drop_kind(&self) -> Option<DropKind> {
        match &self.asked {
            Asked::Declarations(_) => None,
            Asked::Drop(kind, _) => Some(*kind),
        }
    }

    /// The IRI of the predicate or graph a Drop names; `None` otherwise.
    #[must_use]
    pub fn target(&self) -> Option<&str> {
        match &self.asked {
            Asked::Declarations(_) => None,
            Asked::Drop(_, target) => target.as_deref(),
        }
    }

    /// The declarations of `kind` 1, in order; empty for a Drop.
    #[must_use]
    pub fn declared(&self) -> &[Declared] {
        match &self.asked {
            Asked::Declarations(declarations) => declarations,
            Asked::Drop(..) => &[],
        }
    }
}
