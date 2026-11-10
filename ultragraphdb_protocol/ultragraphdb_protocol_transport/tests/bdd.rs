/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The wire-protocol scenario of stage 6 whose decision the transport makes, a Describe's frames spending credit,
//! bound to `features/wire-protocol/`.

use cucumber::{World, then, when};
use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};
use ultragraphdb_protocol_transport::Outbound;
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, Describe, FLAG_LAST, FrameView, Header, MAX_FRAME_SIZE,
};
use ultragraphdb_tool_bdd::TestResult;

/// The test utilities of the unit tests, shared by path.
pub mod utils {
    #[path = "feed.rs"]
    pub mod feed;
    #[path = "peer.rs"]
    pub mod peer;
}

use crate::utils::peer::CLUSTER;

/// The state the scenarios carry from one step to the next.
#[derive(Debug, Default, World)]
pub struct Steps {
    sent: Vec<usize>,
    received: Vec<Result<(), ProtocolError>>,
}

#[when("a Describe answer takes five frames and the Welcome's `initial_credit` is 4")]
fn five_describe_frames(w: &mut Steps) -> TestResult {
    let mut outbound = Outbound::new(4, 4);
    outbound.open(1)?;
    let frames: Vec<Vec<u8>> = (0..5)
        .map(|index| {
            let flags = if index == 4 { FLAG_LAST } else { 0 };
            Describe::new(0, 42).frame(
                Header::new(CLUSTER, Command::Describe)
                    .with_stream(1)
                    .with_flags(flags),
            )
        })
        .collect::<Result<_, _>>()?;
    for frame in &frames {
        outbound.push(1, frame.clone())?;
    }
    let before = std::iter::from_fn(|| outbound.next_frame()).count();
    outbound.grant(1, 1);
    let after = std::iter::from_fn(|| outbound.next_frame()).count();
    w.sent = vec![before, after];
    // The client's side: an initial credit of 4, each frame that spends credit spending 1 (PROTOCOL.md §11.3).
    let mut credit = 4u32;
    w.received = frames
        .iter()
        .map(|frame| {
            let view = FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE)?;
            if view.command().spends_credit() {
                credit = credit
                    .checked_sub(1)
                    .ok_or_else(|| ProtocolError::past_credit(1))?;
            }
            Ok(())
        })
        .collect();
    Ok(())
}

#[then(
    "the fifth frame is sent only after the client's Credit, and a fifth frame sent without one is refused as `PastCredit`"
)]
fn fifth_after_credit(w: &mut Steps) -> TestResult {
    assert_eq!(w.sent, vec![4, 1]);
    let refused: Vec<Option<ProtocolErrorEnum>> = w
        .received
        .iter()
        .map(|r| r.as_ref().err().map(|e| e.0.clone()))
        .collect();
    assert_eq!(
        refused,
        vec![
            None,
            None,
            None,
            None,
            Some(ProtocolErrorEnum::PastCredit { stream: 1 })
        ]
    );
    Ok(())
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
