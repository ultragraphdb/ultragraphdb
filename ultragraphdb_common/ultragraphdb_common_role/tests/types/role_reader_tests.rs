/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_role::RoleClient;
use ultragraphdb_protocol_transport::{BufferPool, Endpoint};
use ultragraphdb_protocol_wire::{BodyBuilder, Command, Header, MAX_FRAME_SIZE, Members};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::serve_tests::{CLUSTER, PASSWORD, bounded, free_tcp, serving};

/// A split reader reads a reply into a pool of the caller's, holding the frame past the next read of its own.
#[test]
fn test_a_reader_reads_into_the_callers_pool() -> TestResult {
    bounded(async {
        let address = serving(&free_tcp()?, Endpoint::Frontend).await?;
        let client = RoleClient::dial(&address, CLUSTER, "storage", PASSWORD).await?;
        let (mut reader, mut writer, _welcome) = client.split();
        let pool = BufferPool::new(2, MAX_FRAME_SIZE);
        let mut held = Vec::new();
        for stream in 1..=2 {
            let header = Header::new(CLUSTER, Command::Members).with_stream(stream);
            writer.send(Members::new().frame(header)?).await?;
            held.push(reader.next_frame_in(&pool).await?);
        }
        let streams: Vec<u64> = held.iter().map(|frame| frame.stream()).collect();
        assert_eq!(streams, [1, 2]);
        assert!(
            held.iter()
                .all(|frame| frame.command() == Command::Members.number())
        );
        Ok(())
    })
}
