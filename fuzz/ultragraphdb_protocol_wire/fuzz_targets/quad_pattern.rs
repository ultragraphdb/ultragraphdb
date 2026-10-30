/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Quad patterns with parameters: any bytes as a Prepare body, then as an Execute body, through the statement cache.
//! Nothing panics; a prepared statement binds exactly its parameters, and the bound pattern holds no parameter.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_protocol_wire::{
    BodyView, Command, ExecuteView, FrameView, Header, MAX_FRAME_SIZE, PatternSlot, PrepareView,
    StatementCache, StatementError, StatementErrorEnum,
};

const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

fuzz_target!(|data: &[u8]| {
    // A little-endian u16 splits the input: the Prepare body, then the Execute body.
    let Some((len, rest)) = data.split_first_chunk::<2>() else {
        return;
    };
    let split = usize::from(u16::from_le_bytes(*len)).min(rest.len());
    let (prepare, execute) = rest.split_at(split);
    let Ok(prepare) = Header::new(CLUSTER, Command::Prepare).encode_frame(prepare) else {
        return;
    };
    let frame =
        FrameView::validate(&prepare, CLUSTER, MAX_FRAME_SIZE).expect("a built frame validates");
    let Ok(request) = PrepareView::from_frame(&frame) else {
        return;
    };
    let mut cache = StatementCache::new(4);
    let Ok(reply) = cache.prepare(&request, 5) else {
        return;
    };
    assert_eq!(
        reply.parameter_types().len(),
        request.pattern().parameters()
    );
    let Ok(execute) = Header::new(CLUSTER, Command::Execute).encode_frame(execute) else {
        return;
    };
    let frame =
        FrameView::validate(&execute, CLUSTER, MAX_FRAME_SIZE).expect("a built frame validates");
    let Ok(view) = ExecuteView::from_frame(&frame) else {
        return;
    };
    match cache.execute(&view, 5) {
        Ok((terms, bound)) => {
            assert_eq!(view.statement(), reply.statement());
            assert_eq!(view.parameters().len(), reply.parameter_types().len());
            assert_eq!(bound.parameters(), 0);
            // Every slot is unbound, the default graph, or a term of the merged table.
            assert!(bound.slots().iter().all(|s| match s {
                PatternSlot::Parameter(_) => false,
                PatternSlot::Bound(i) => (*i as usize) < terms.len(),
                _ => true,
            }));
            assert!(terms.len() >= reply.terms().len());
        }
        Err(StatementError(StatementErrorEnum::Unknown { statement })) => {
            assert_ne!(statement, reply.statement());
        }
        Err(StatementError(StatementErrorEnum::Parameters {
            expected, found, ..
        })) => {
            assert_ne!(expected, found);
        }
        Err(other) => panic!("an execution at the statement's own schema id is refused: {other}"),
    }
    assert!(cache.execute(&view, 6).is_err());
});
