/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A resume key's last row by its terms' text (`PROTOCOL.md` §11.8): a read from the key goes on after that row, and a key whose
//! terms its homes do not hold is refused.

use ultragraphdb_common_compute::read_times_of;
use ultragraphdb_protocol_wire::{ErrorCode, ReadLevel, ResumeKey, ResumeRow, TermTable};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::played_answers::{Pattern, answer_on, pattern_hash, read_of, snapshot};
use crate::utils::played_cluster::{
    EPOCH, Played, ROWS_PREDICATE, cells_of, hold_rows, run, short_rows, version,
};

const P: &str = ROWS_PREDICATE;

/// A read from a resume key returns the rows after the key's, and a key whose terms its partitions do not hold is refused.
#[test]
fn test_a_read_resumed_goes_on_after_the_keys_row() -> TestResult {
    run(async {
        let played = Played::start(1, read_times_of(None, None)).await?;
        let rows = short_rows(3);
        let _ids = hold_rows(played.partition(1)?, &rows)?;
        let mut client = played.client().await?;
        let pattern = Pattern::of(None, Some(P), None);
        let first = answer_on(&mut client, &snapshot(&pattern)?.with_limit(1)).await?;
        let resume = first.batches.last().context("a batch")?.resume.clone();
        let read = read_of(&pattern, true, ReadLevel::Snapshot, resume)?
            .with_read_version(version()?)
            .with_epoch(EPOCH);
        let rest = answer_on(&mut client, &read).await?.whole()?;
        assert_eq!(rest, cells_of(rows.get(1..).context("the rest")?));
        Ok(())
    })
}

/// A resume key naming a subject the partition's dictionary lacks is refused with Error 1.
#[test]
fn test_a_resume_key_of_an_unheld_term_is_refused() -> TestResult {
    run(async {
        let played = Played::start(1, read_times_of(None, None)).await?;
        let rows = short_rows(2);
        let _ids = hold_rows(played.partition(1)?, &rows)?;
        let pattern = Pattern::of(None, Some(P), None);
        let read = snapshot(&pattern)?;
        let mut table = TermTable::new();
        let (s, p, o) = (
            table.iri("http://ex.org/unheld"),
            table.iri(P),
            table.iri("http://ex.org/o0"),
        );
        let resume = ResumeKey::new(
            pattern_hash(&read)?,
            read.read_version(),
            table,
            ResumeRow::new(None, s, p, o, None),
        )
        .encode();
        let resumed = read_of(&pattern, true, ReadLevel::Snapshot, resume)?
            .with_read_version(read.read_version())
            .with_epoch(read.epoch());
        let mut client = played.client().await?;
        let answered = answer_on(&mut client, &resumed).await?;
        assert_eq!(answered.refusal()?.code, ErrorCode::Protocol);
        Ok(())
    })
}
