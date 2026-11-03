/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU32;

use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Decision, ErrorCode, RoundBody, RoundHeader, RoundView,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::write_set_record::write_set_record_tests::{FIRST_VERSION, records};
use crate::utils::golden_bodies::{frame_of, golden};

/// The golden Round's parent checksum.
pub const PARENT: u64 = 0x0F0E_0D0C_0B0A_0908;

/// Derivation 1, the golden Round's.
pub fn first_derivation() -> TestResult<NonZeroU32> {
    NonZeroU32::new(1).context("derivation 1")
}

/// The golden Round, as generate_bodies.py lays it out.
pub fn golden_round() -> TestResult<RoundBody> {
    let [transaction, home, build, drop] = records()?;
    let mut round = RoundBody::new(3, PARENT, 1, FIRST_VERSION, first_derivation()?);
    round.commit(transaction.encode());
    round.abort(ErrorCode::Conflict, 1);
    round.commit(home.encode());
    round.commit(build.encode());
    round.commit(drop.encode());
    // Inserted out of order: the round keeps them in ascending key order.
    round.insert_dictionary(b"\x21dict-c".to_vec(), b"value-c".to_vec());
    round.insert_dictionary(b"\x20dict-a".to_vec(), b"value-a".to_vec());
    round.insert_dictionary(b"\x20dict-b".to_vec(), Vec::new());
    Ok(round)
}

#[test]
fn test_the_golden_round_builds_the_golden_frame() -> TestResult {
    let round = golden_round()?;
    assert_eq!(
        round.header(),
        RoundHeader::new(3, PARENT, 1, FIRST_VERSION, 5, first_derivation()?)
    );
    assert_eq!(
        round.decisions(),
        &[
            Decision::Committed,
            Decision::Aborted {
                code: ErrorCode::Conflict,
                detail: 1
            },
            Decision::Committed,
            Decision::Committed,
            Decision::Committed,
        ]
    );
    let encoded: Vec<Vec<u8>> = records()?.iter().map(|r| r.encode()).collect();
    assert_eq!(round.records(), &encoded[..]);
    assert_eq!(
        round.dictionary().keys().cloned().collect::<Vec<_>>(),
        vec![
            b"\x20dict-a".to_vec(),
            b"\x20dict-b".to_vec(),
            b"\x21dict-c".to_vec()
        ]
    );
    assert_eq!(round.command_header(), round.header().encode());
    assert_eq!(
        round.frame(golden("round")?.header())?,
        golden("round")?.bytes()?
    );
    Ok(())
}

#[test]
fn test_a_dictionary_key_inserted_again_keeps_its_last_value() -> TestResult {
    let mut round = RoundBody::new(1, 0, 1, FIRST_VERSION, first_derivation()?);
    round.insert_dictionary(b"k".to_vec(), b"first".to_vec());
    round.insert_dictionary(b"k".to_vec(), b"second".to_vec());
    assert_eq!(round.dictionary().len(), 1);
    assert_eq!(round.dictionary().get(&b"k"[..]), Some(&b"second".to_vec()));
    Ok(())
}

#[test]
fn test_an_empty_round_has_no_entries_and_reads_back() -> TestResult {
    let round = RoundBody::new(1, 0, 1, FIRST_VERSION, first_derivation()?);
    assert_eq!(round.header().transactions(), 0);
    assert!(round.decisions().is_empty());
    assert!(round.records().is_empty());
    assert!(round.dictionary().is_empty());
    // Four empty sections: a 32-byte table and nothing after it.
    assert_eq!(round.body().len(), 32);
    let bytes = round.frame(golden("round")?.header())?;
    let view = RoundView::from_frame(&frame_of(&bytes)?)?;
    assert!(view.records().is_empty());
    assert!(view.dictionary().is_empty());
    assert_eq!(view.to_body()?, round);
    Ok(())
}
