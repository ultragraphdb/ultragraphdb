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
use crate::types::request_term_view::RequestTermView;
use crate::types::term_table::TermTable;

/// A statement the cache holds: its pattern, its namespace, the schema id it was prepared against and its types.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Prepared {
    namespace: u64,
    terms: TermTable,
    pattern: QuadPattern,
    schema: u64,
    parameter_types: Vec<ColumnType>,
}

/// A connection's prepared statements, by id: prepared once, executed with parameters bound by position, refused as
/// stale once their namespace's schema id no longer matches. Ids start at 1 and are never reused on the connection;
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
        let pattern = *request.pattern();
        let declared = request.parameter_types();
        let types = QuadPattern::slot_types();
        let parameter_slots = pattern
            .slots()
            .into_iter()
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
            .into_iter()
            .zip(types)
            .filter(|(slot, _)| matches!(slot, PatternSlot::Unbound))
            .map(|(_, slot_type)| slot_type)
            .collect();
        let statement = self.next;
        self.next = self.next.saturating_add(1);
        let terms = request.terms().to_table();
        self.statements.insert(
            statement,
            Prepared {
                namespace: request.namespace(),
                terms: terms.clone(),
                pattern,
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
            terms,
            pattern,
            declared.to_vec(),
            result_types,
        ))
    }

    /// The pattern of `request`'s statement with its parameters bound, and the term table its indices name: the
    /// statement's terms, then the parameters' from the execution's table, each once. Given when `schema` is the
    /// namespace's current schema id; refused, in this order: [`StatementError::Unknown`]; [`StatementError::Stale`]
    /// when the statement was prepared against another schema id; [`StatementError::Parameters`] when the parameters
    /// are not one per parameter slot; [`StatementError::NotANode`] when a literal or a triple term is bound at a slot
    /// taking a node.
    pub fn execute(
        &self,
        request: &ExecuteView<'_>,
        schema: u64,
    ) -> Result<(TermTable, QuadPattern), StatementError> {
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
        let mismatch = StatementError::Parameters(statement, expected, parameters.len());
        if parameters.len() != expected {
            return Err(mismatch);
        }
        let p = prepared.pattern;
        let node_parameters = p
            .slots()
            .into_iter()
            .zip(QuadPattern::slot_types())
            .filter_map(|(slot, slot_type)| match slot {
                PatternSlot::Parameter(n) if slot_type == ColumnType::Node => Some(n as usize),
                _ => None,
            });
        for n in node_parameters {
            let node = parameters
                .get(n)
                .and_then(|index| request.terms().get(*index))
                .is_some_and(|term| {
                    matches!(
                        term,
                        RequestTermView::Iri(_) | RequestTermView::BlankNode(_)
                    )
                });
            if !node {
                return Err(StatementError::NotANode(statement, n));
            }
        }
        let mut terms = prepared.terms.clone();
        let bound = parameters
            .iter()
            .map(|p| terms.import(request.terms(), *p))
            .collect::<Option<Vec<u32>>>()
            .ok_or(mismatch)?;
        let bind = |slot: PatternSlot| match slot {
            PatternSlot::Parameter(n) => bound
                .get(n as usize)
                .map_or(PatternSlot::Unbound, |index| PatternSlot::Bound(*index)),
            other => other,
        };
        let pattern = QuadPattern::new(
            bind(p.graph()),
            bind(p.subject()),
            bind(p.predicate()),
            bind(p.object()),
        );
        Ok((terms, pattern))
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
