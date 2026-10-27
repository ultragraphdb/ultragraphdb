/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use crate::errors::statement_error::StatementError;
use crate::types::column_type::ColumnType;
use crate::types::execute_view::ExecuteView;
use crate::types::pattern_slot::PatternSlot;
use crate::types::prepare::Prepare;
use crate::types::prepare_view::PrepareView;
use crate::types::quad_pattern::QuadPattern;

/// A statement the cache holds: its pattern, its namespace, the schema id it was prepared against and its types.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Prepared {
    namespace: u64,
    pattern: QuadPattern<String>,
    schema: u64,
    parameter_types: Vec<ColumnType>,
}

/// A session's prepared statements, by id: prepared once, executed with parameters bound by position, refused as
/// stale once their namespace's schema id no longer matches. Ids start at 1 and are never reused within a session;
/// past `capacity` statements the oldest is evicted, and executing it is refused as unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementCache {
    statements: BTreeMap<u64, Prepared>,
    next: u64,
    capacity: usize,
}

impl StatementCache {
    /// A cache of at most `capacity` statements, at least one.
    pub fn new(capacity: usize) -> Self {
        Self {
            statements: BTreeMap::new(),
            next: 1,
            capacity: capacity.max(1),
        }
    }

    /// Prepares `request` against the namespace's current `schema` id: each declared parameter type fits its slot
    /// (a node where a node is expected, refused as [`StatementError::ParameterType`]); answered with the statement's
    /// id, the schema id, the parameter types and a result column type per unbound slot.
    pub fn prepare(
        &mut self,
        request: &PrepareView<'_>,
        schema: u64,
    ) -> Result<Prepare, StatementError> {
        let pattern = request.pattern();
        let declared = request.parameter_types();
        let types = QuadPattern::<&str>::slot_types();
        let parameter_slots = pattern
            .slots()
            .iter()
            .zip(types)
            .filter(|(slot, _)| matches!(slot, PatternSlot::Parameter(_)))
            .map(|(_, slot_type)| slot_type)
            .collect::<Vec<_>>();
        for (parameter, (declared, slot)) in declared.iter().zip(&parameter_slots).enumerate() {
            if *slot == ColumnType::Node && *declared != ColumnType::Node {
                return Err(StatementError::ParameterType(parameter, *declared, *slot));
            }
        }
        let result_types = pattern
            .slots()
            .iter()
            .zip(types)
            .filter(|(slot, _)| matches!(slot, PatternSlot::Unbound))
            .map(|(_, slot_type)| slot_type)
            .collect();
        let statement = self.next;
        self.next = self.next.saturating_add(1);
        let owned = pattern.to_owned_pattern();
        self.statements.insert(
            statement,
            Prepared {
                namespace: request.namespace(),
                pattern: owned.clone(),
                schema,
                parameter_types: declared.to_vec(),
            },
        );
        while self.statements.len() > self.capacity {
            self.statements.pop_first();
        }
        Ok(Prepare::new(
            request.namespace(),
            statement,
            schema,
            owned,
            declared.to_vec(),
            result_types,
        ))
    }

    /// The pattern of `request`'s statement with its parameters bound, when `schema` is the namespace's current
    /// schema id. Refused, in this order: [`StatementError::Unknown`]; [`StatementError::Stale`] when the statement
    /// was prepared against another schema id; [`StatementError::Parameters`] when the parameters are not one per
    /// parameter slot.
    pub fn execute(
        &self,
        request: &ExecuteView<'_>,
        schema: u64,
    ) -> Result<QuadPattern<String>, StatementError> {
        let statement = request.statement();
        let prepared = self
            .statements
            .get(&statement)
            .ok_or(StatementError::Unknown(statement))?;
        if prepared.schema != schema {
            return Err(StatementError::Stale(statement, prepared.schema, schema));
        }
        let parameters = request.parameters();
        let expected = prepared.parameter_types.len();
        if parameters.len() != expected {
            return Err(StatementError::Parameters(
                statement,
                expected,
                parameters.len(),
            ));
        }
        let bind = |slot: &PatternSlot<String>| match slot {
            PatternSlot::Parameter(n) => parameters
                .get(*n as usize)
                .map_or(PatternSlot::Unbound, |p| PatternSlot::Bound(p.to_string())),
            other => other.clone(),
        };
        let p = &prepared.pattern;
        Ok(QuadPattern::new(
            bind(p.graph()),
            bind(p.subject()),
            bind(p.predicate()),
            bind(p.object()),
        ))
    }

    /// The namespace statement `statement` reads, `None` when the cache does not hold it.
    pub fn namespace(&self, statement: u64) -> Option<u64> {
        self.statements.get(&statement).map(|p| p.namespace)
    }

    pub fn len(&self) -> usize {
        self.statements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.statements.is_empty()
    }
}
