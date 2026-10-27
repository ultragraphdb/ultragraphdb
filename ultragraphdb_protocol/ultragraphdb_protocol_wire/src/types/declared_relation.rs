/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A relation's declaration, by IRI text: its roles.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeclaredRelation {
    iri: String,
    roles: Vec<String>,
}

impl DeclaredRelation {
    pub fn new(iri: impl Into<String>, roles: Vec<String>) -> Self {
        Self {
            iri: iri.into(),
            roles,
        }
    }

    pub fn iri(&self) -> &str {
        &self.iri
    }

    pub fn roles(&self) -> &[String] {
        &self.roles
    }
}
