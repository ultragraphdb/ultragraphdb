/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{ClientHandshake, ServerHandshake, TransportError};
use ultragraphdb_protocol_wire::{Command, HeaderView, Hello, MAX_FRAME_SIZE, Welcome};
use ultragraphdb_tool_bdd::{Context, TestResult};

const CLUSTER: u64 = 3;

#[test]
fn test_an_answer_gives_its_frame_and_outcome_and_parts_alike() -> TestResult {
    let hello = ClientHandshake::new(CLUSTER, Hello::new(1, 1, 0, MAX_FRAME_SIZE as u32, "c"))
        .hello_frame()?;
    let answer = ServerHandshake::new(
        CLUSTER,
        Welcome::new(1, 0, "s")
            .with_max_frame(1 << 20)
            .with_max_streams(1)
            .with_initial_credit(0),
    )
    .receive(&hello);
    let (frame, outcome) = answer.clone().into_parts();
    assert_eq!(answer.frame(), frame.as_deref());
    assert_eq!(answer.outcome(), &outcome);
    let frame = frame.context("the Welcome's frame")?;
    assert_eq!(
        HeaderView::new(&frame)?.command(),
        Command::Welcome.number()
    );
    assert_eq!(outcome?.max_streams(), 1);
    // A refusal's parts: the Error frame and the refusal, alike.
    let refused = ServerHandshake::new(CLUSTER, Welcome::new(1, 0, "s")).receive(&[0; 8]);
    let (refusal_frame, refusal_outcome) = refused.clone().into_parts();
    assert_eq!(refused.frame(), refusal_frame.as_deref());
    assert_eq!(refused.outcome(), &refusal_outcome);
    assert_eq!(
        HeaderView::new(&refusal_frame.context("the Error frame")?)?.command(),
        Command::Error.number()
    );
    assert_eq!(
        refusal_outcome.err().context("the refusal")?,
        TransportError::protocol(ProtocolError::short_header(8))
    );
    Ok(())
}
