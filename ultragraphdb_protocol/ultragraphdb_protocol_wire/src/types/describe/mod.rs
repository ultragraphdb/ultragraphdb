/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod describe_body_builder;

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::declared_entity_type::DeclaredEntityType;
use crate::types::declared_predicate::DeclaredPredicate;
use crate::types::declared_relation::DeclaredRelation;
use crate::types::describe_entry::DescribeEntry;
use crate::types::header::Header;
use crate::utils::checksum::xxh3_64;
use crate::utils::continuation::{Cut, continuation_frames};

/// What a tenant holds, the data dictionary's answer (`PROTOCOL.md` §8.3): sent with the tenant alone, answered with the
/// snapshot it was read at, the labels, relationship types and property keys, the catalog's declarations (predicates,
/// entity types and relations) and the entries of the catalog version they are drawn from. An entity type's preorder
/// number is held beside it until the preorder numbering is removed; the wire carries none, a record's `extra` being
/// 0 (§4.6).
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `tenant` | u64 |
/// | 8 | `read_version` | u64 |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | strings: ends |
/// | 1 | strings: text, each IRI plain |
/// | 2 | labels, u32 string indices |
/// | 3 | relationship types, u32 string indices |
/// | 4 | property keys, u32 string indices |
/// | 5 | declarations: change records of kinds 8, 9 and 10, `extra` 0 |
/// | 6 | the declarations' args |
/// | 7 | entries, 16 bytes each ([`DescribeEntry`]), by kind, then by name |
/// | 8 | the entries' bytes, back to back in entry order |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Describe {
    tenant: u64,
    read_version: u64,
    labels: Vec<String>,
    relationship_types: Vec<String>,
    property_keys: Vec<String>,
    predicates: Vec<DeclaredPredicate>,
    entity_types: Vec<(DeclaredEntityType, u32)>,
    relations: Vec<DeclaredRelation>,
    entries: Vec<DescribeEntry>,
}

impl Describe {
    /// An answer for `tenant` read at `read_version`, empty until its parts are set.
    pub fn new(tenant: u64, read_version: u64) -> Self {
        Self {
            tenant,
            read_version,
            labels: Vec::new(),
            relationship_types: Vec::new(),
            property_keys: Vec::new(),
            predicates: Vec::new(),
            entity_types: Vec::new(),
            relations: Vec::new(),
            entries: Vec::new(),
        }
    }

    /// The tenant described.
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The snapshot the answer was read at; zero in a request.
    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    /// The labels.
    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    /// The relationship types.
    pub fn relationship_types(&self) -> &[String] {
        &self.relationship_types
    }

    /// The property keys.
    pub fn property_keys(&self) -> &[String] {
        &self.property_keys
    }

    /// The declared predicates.
    pub fn predicates(&self) -> &[DeclaredPredicate] {
        &self.predicates
    }

    /// The declared entity types, each with its preorder number.
    pub fn entity_types(&self) -> &[(DeclaredEntityType, u32)] {
        &self.entity_types
    }

    /// The declared relations.
    pub fn relations(&self) -> &[DeclaredRelation] {
        &self.relations
    }

    /// The answer with its labels.
    pub fn with_labels(mut self, labels: Vec<String>) -> Self {
        self.labels = labels;
        self
    }

    /// The answer with its relationship types.
    pub fn with_relationship_types(mut self, relationship_types: Vec<String>) -> Self {
        self.relationship_types = relationship_types;
        self
    }

    /// The answer with its property keys.
    pub fn with_property_keys(mut self, property_keys: Vec<String>) -> Self {
        self.property_keys = property_keys;
        self
    }

    /// The answer with its declared predicates.
    pub fn with_predicates(mut self, predicates: Vec<DeclaredPredicate>) -> Self {
        self.predicates = predicates;
        self
    }

    /// The entity types, each with its preorder number.
    pub fn with_entity_types(mut self, entity_types: Vec<(DeclaredEntityType, u32)>) -> Self {
        self.entity_types = entity_types;
        self
    }

    /// The answer with its declared relations.
    pub fn with_relations(mut self, relations: Vec<DeclaredRelation>) -> Self {
        self.relations = relations;
        self
    }

    /// A request for `tenant`'s description.
    pub fn request(tenant: u64) -> Self {
        Self::new(tenant, 0)
    }

    /// The schema id of the declarations and entries (`PROTOCOL.md` §11.7): XXH3-64 of the body of this answer with
    /// its labels, relationship types and property keys left out, so it changes with the catalog and not with the
    /// data. A prepared statement's schema id.
    pub fn schema_id(&self) -> u64 {
        let declarations = Describe::new(0, 0)
            .with_predicates(self.predicates.clone())
            .with_entity_types(self.entity_types.clone())
            .with_relations(self.relations.clone())
            .with_entries(self.entries.clone());
        xxh3_64(&declarations.body())
    }

    /// The answer with the entries of the catalog version its declarations are drawn from, by kind, then by name, the
    /// installed sources and the retired names among them.
    #[must_use]
    pub fn with_entries(self, entries: Vec<DescribeEntry>) -> Self {
        Self { entries, ..self }
    }

    /// The entries.
    #[must_use]
    pub fn entries(&self) -> &[DescribeEntry] {
        &self.entries
    }

    /// The answer as the frames that carry it within `max_frame` (`PROTOCOL.md` §8.3, *Continuation*): one frame when
    /// it fits, else as many as it takes, each with this command header, each section split only between whole
    /// records or u32s and the strings' text only on a character boundary, the last alone flagged `LAST`. `header`
    /// gives every frame's other fields.
    ///
    /// # Errors
    ///
    /// [`ultragraphdb_error_protocol::ProtocolError::frame_too_large`] when a frame within `max_frame` holds no unit of
    /// what is left of the answer, naming the least frame that holds one.
    pub fn frames(&self, header: Header, max_frame: usize) -> Result<Vec<Vec<u8>>, ProtocolError> {
        let layout = describe_body_builder::layout(self);
        let [s0, s1, s2, s3, s4, s5, s6, s7, s8] = &layout.sections;
        // Section 0's ends, 2 to 4's indices and 6's args are u32s; 5's and 7's records 16 bytes; 1 the strings'
        // text; 8 the entries' bytes, a source's text among them.
        let sections = [
            (s0.as_slice(), Cut::Units(4)),
            (s1.as_slice(), Cut::Text),
            (s2.as_slice(), Cut::Units(4)),
            (s3.as_slice(), Cut::Units(4)),
            (s4.as_slice(), Cut::Units(4)),
            (s5.as_slice(), Cut::Units(16)),
            (s6.as_slice(), Cut::Units(4)),
            (s7.as_slice(), Cut::Units(16)),
            (s8.as_slice(), Cut::Texts(&layout.texts)),
        ];
        let header = header
            .with_command(Command::Describe)
            .with_command_header(self.command_header());
        continuation_frames(header, max_frame, &sections)
    }
}
