/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Golden vectors from `tools/golden/chunker.go`: NQuadBuffer.ParseJSON.

use std::collections::BTreeSet;

use ultragraphdb_core_chunker::{JsonOp, parse_json};

use crate::golden_nquad::{JsonCase, canonical, golden, nquad_json};

fn check_json(c: &JsonCase) -> Result<(), String> {
    let op = if c.op == 0 {
        JsonOp::Set
    } else {
        JsonOp::Delete
    };
    match parse_json(c.input.as_bytes(), op) {
        Ok((nqs, md)) => {
            if !c.err.is_empty() {
                return Err(format!("{} ({op:?}): {nqs:?}, want {:?}", c.name, c.err));
            }
            let got: Vec<_> = nqs.iter().map(|nq| nquad_json(nq, true)).collect();
            let blanks: BTreeSet<&str> = got
                .iter()
                .flat_map(|nq| [nq.subject.as_str(), nq.object_id.as_str()])
                .filter(|s| s.starts_with("_:dg."))
                .collect();
            if canonical(got.clone()) != canonical(c.nquads.clone()) || blanks.len() != c.blanks {
                return Err(format!("{} ({op:?}): {got:?}, want {:?}", c.name, c.nquads));
            }
            let mut hints: Vec<(String, i32)> = md.pred_hints.into_iter().collect();
            hints.sort();
            let want: Vec<(String, i32)> =
                c.hints.iter().map(|h| (h.pred.clone(), h.hint)).collect();
            if hints != want {
                return Err(format!(
                    "{} ({op:?}): hints {hints:?}, want {want:?}",
                    c.name
                ));
            }
            Ok(())
        }
        // serde_json words its syntax errors its own way; every other error has Dgraph's text, one of those Go gave
        // over its runs.
        Err(_) if c.err_json => Ok(()),
        Err(e) if c.errs.contains(&e.to_string()) => Ok(()),
        Err(e) => Err(format!("{} ({op:?}): {e}, want {:?}", c.name, c.errs)),
    }
}

#[test]
fn parse_json_matches_go() {
    let g = golden::<JsonCase>("chunker_json");
    assert!(g.cases().len() > 250);
    assert!(
        g.cases()
            .iter()
            .filter(|c| c.err.is_empty())
            .all(|c| c.stable)
    );
    let failures: Vec<String> = g
        .cases()
        .iter()
        .filter_map(|c| check_json(c).err())
        .collect();
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
