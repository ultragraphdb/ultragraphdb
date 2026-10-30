/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Execute, TermTable};

use crate::utils::golden_bodies::golden;

pub fn golden_execute() -> Execute {
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    Execute::new(1, 5, 0, terms, vec![a], Vec::new())
}

#[test]
fn test_execute_builds_the_golden_frame() {
    let execute = golden_execute();
    assert_eq!(
        (execute.statement(), execute.transaction(), execute.limit()),
        (1, 5, 0)
    );
    assert!(!execute.statement_ids());
    assert_eq!(execute.parameters(), &[0]);
    assert_eq!(execute.terms().len(), 1);
    assert!(execute.resume().is_empty());
    assert_eq!(
        execute.frame(golden("execute").header()).unwrap(),
        golden("execute").bytes()
    );
}

#[test]
fn test_an_execution_lays_out_its_header_by_the_table() {
    // statement at 0, transaction at 8, limit at 16, flags at 20.
    let execute = Execute::new(3, 7, 9, TermTable::new(), Vec::new(), Vec::new());
    let header = execute.command_header();
    assert_eq!(&header[..8], &3u64.to_le_bytes());
    assert_eq!(&header[8..16], &7u64.to_le_bytes());
    assert_eq!(&header[16..20], &9u32.to_le_bytes());
    assert_eq!(&header[20..24], &[0, 0, 0, 0]);
    let asked = execute.with_statement_ids();
    assert!(asked.statement_ids());
    assert_eq!(&asked.command_header()[20..24], &[1, 0, 0, 0]);
    assert!(asked.command_header()[24..].iter().all(|b| *b == 0));
}
