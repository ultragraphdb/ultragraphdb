/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A Declare is one frame: law L5.13 of `notes/laws.md` (`PROTOCOL.md` §11.13, *A new version*).

use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_common_frontend::declare_fits;
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, Declare, Declared, DeclaredPredicate, Header, MAX_FRAME_FLOOR_U32,
};

/// L5.13: declarations whose Declare frame passes `max_frame` are refused with code 6, `detail` `max_frame`, and those whose
/// frame fits are taken: no Declare built from what is taken is larger than `max_frame`.
#[hegel::test(test_cases = 40)]
fn test_a_declare_is_refused_exactly_past_its_frame(tc: TestCase) {
    let count = tc.draw(gs::integers::<usize>().min_value(1).max_value(40));
    let length = tc.draw(gs::integers::<usize>().min_value(1).max_value(20_000));
    let declared: Vec<Declared> = (0..count)
        .map(|at| {
            Declared::Predicate(DeclaredPredicate::new(format!(
                "http://ex.org/{at}/{}",
                "p".repeat(length)
            )))
        })
        .collect();
    let max_frame = MAX_FRAME_FLOOR_U32;
    let built = Declare::declarations(1, 3, 41, declared.clone())
        .frame(Header::new(7, Command::Declare).with_stream(1))
        .map(|frame| frame.len());
    let fits = built.as_ref().is_ok_and(|len| *len <= max_frame as usize);
    match declare_fits(1, 3, 41, &declared, max_frame) {
        Ok(()) => assert!(fits, "taken, the frame fits"),
        Err(refusal) => {
            assert!(!fits, "refused, the frame does not fit");
            assert_eq!(refusal.code().number(), 6);
            assert_eq!(refusal.detail(), u64::from(max_frame));
        }
    }
}
