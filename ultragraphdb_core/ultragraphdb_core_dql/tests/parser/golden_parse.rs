/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The golden cases of `tools/golden/dql.go`, `dql_parse.json`: each input with the distinct outcomes of Go's 41
//! runs, compared with the port's, and with the cases [`DEVIATIONS`] lists. The unit tests and the BDD suite share
//! it.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value as Json;
use ultragraphdb_core_dql::{DqlError, Request, parse};
use ultragraphdb_test_bdd::Golden;

use super::parser_deviations::DEVIATIONS;
use super::tree_json;

/// One run's result: the tree as JSON, or the error text.
#[derive(Debug, Deserialize)]
pub(crate) struct Run {
    ok: Json,
    err: String,
    is_err: bool,
}

/// The distinct results of Go's runs of one parse, in the order first seen. There are several where Go's map
/// iteration order picks which failure it reports (ledger 12).
#[derive(Debug, Deserialize)]
pub(crate) struct Outcome {
    outcomes: Vec<Run>,
}

impl Outcome {
    /// Whether the port's result is one of Go's.
    pub(crate) fn admits(&self, got: &Result<Json, DqlError>) -> bool {
        self.outcomes.iter().any(|r| match got {
            Ok(v) => !r.is_err && r.ok == *v,
            Err(e) => r.is_err && r.err == e.to_string(),
        })
    }

    /// Whether some run of Go's produced a tree.
    pub(crate) fn has_tree(&self) -> bool {
        self.outcomes.iter().any(|r| !r.is_err)
    }

    /// Go's results, for a failure message.
    pub(crate) fn describe(&self) -> String {
        self.outcomes
            .iter()
            .map(|r| {
                if r.is_err {
                    format!("error {:?}", r.err)
                } else {
                    r.ok.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

/// One input: parsed as a query (with the variables, when given), as a mutation and as a DQL request. The last
/// two are absent for an input given with variables, which those parsers do not take.
#[derive(Debug, Deserialize)]
pub(crate) struct Case {
    pub(crate) input: String,
    pub(crate) variables: Option<HashMap<String, String>>,
    pub(crate) parse: Outcome,
    pub(crate) mutation: Option<Outcome>,
    pub(crate) dql: Option<Outcome>,
}

pub(crate) fn golden() -> Golden<Case> {
    let path = ultragraphdb_test_bdd::testdata_dir().join("golden/dql_parse.json");
    ultragraphdb_test_bdd::load_golden::<Case>(&path)
}

pub(crate) fn request(c: &Case) -> Request {
    match &c.variables {
        Some(vars) => Request::with_variables(c.input.clone(), vars.clone()),
        None => Request::new(c.input.clone()),
    }
}

/// The case's entry in [`DEVIATIONS`]: its ledger entry and the port's error.
fn deviation(c: &Case) -> Option<(u32, &'static str)> {
    let same_vars = |vars: &[(&str, &str)]| match &c.variables {
        None => vars.is_empty(),
        Some(m) => {
            m.len() == vars.len() && vars.iter().all(|(k, v)| m.get(*k).is_some_and(|x| x == v))
        }
    };
    DEVIATIONS
        .iter()
        .filter(|(input, _, _)| *input == c.input)
        .find_map(|(_, ledger, cases)| {
            cases
                .iter()
                .find(|(vars, _)| same_vars(vars))
                .map(|(_, err)| (*ledger, *err))
        })
}

/// The number of cases [`DEVIATIONS`] lists.
pub(crate) fn deviations_listed() -> usize {
    DEVIATIONS.iter().map(|(_, _, cases)| cases.len()).sum()
}

/// How the port's parse of a case compares with Go's.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Compared {
    /// The port's outcome is one of Go's.
    AsGo,
    /// The case is listed in [`DEVIATIONS`], the port gives the listed error, and no run of Go's gave it.
    Deviation,
}

/// Parses the case as a query and compares the outcome with Go's, or with its [`DEVIATIONS`] entry.
pub(crate) fn check_parse(c: &Case) -> Result<Compared, String> {
    let got = parse(&request(c)).map(|r| tree_json::result(&r));
    let failure = |what: &str| {
        format!(
            "parse {:?} with {:?}: {what}\n  Go:   {}\n  port: {}",
            c.input,
            c.variables,
            c.parse.describe(),
            match &got {
                Ok(v) => v.to_string(),
                Err(e) => format!("error {:?}", e.to_string()),
            }
        )
    };
    match deviation(c) {
        Some((ledger, want)) => match &got {
            Err(e) if e.to_string() == want && !c.parse.admits(&got) => Ok(Compared::Deviation),
            Err(e) if e.to_string() == want => Err(failure(&format!(
                "listed as a deviation (ledger {ledger}), but Go gives the same error"
            ))),
            _ => Err(failure(&format!(
                "listed as a deviation (ledger {ledger}) with the error {want:?}"
            ))),
        },
        None if c.parse.admits(&got) => Ok(Compared::AsGo),
        None => Err(failure("not one of Go's outcomes")),
    }
}
