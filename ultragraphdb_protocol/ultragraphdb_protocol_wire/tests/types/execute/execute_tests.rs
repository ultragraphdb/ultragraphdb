/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Execute, TermTable};

use crate::utils::golden_bodies::golden;

pub fn golden_execute() -> Execute {
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    Execute::new(1, 0, terms, vec![a], Vec::new())
}

#[test]
fn test_execute_builds_the_golden_frame() {
    let execute = golden_execute();
    assert_eq!((execute.statement(), execute.limit()), (1, 0));
    assert_eq!(execute.parameters(), &[0]);
    assert_eq!(execute.terms().len(), 1);
    assert!(execute.resume().is_empty());
    assert_eq!(
        execute.frame(golden("execute").header()).unwrap(),
        golden("execute").bytes()
    );
}
