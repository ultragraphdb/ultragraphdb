/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_bus::{BufferPool, Connection, NetworkFaults, SimNetwork};
use ultragraphdb_protocol_wire::{BodyBuilder, Command, Credit, FLAG_LAST, Header};

use crate::utils::ready::ready;

#[test]
fn test_a_received_frame_reads_its_header_and_validates_its_body() {
    let pool = BufferPool::new(2, 1 << 20);
    let network = SimNetwork::new(1, NetworkFaults::none());
    let (client, server) = network.connect();
    let mut client = Connection::new(client, 7);
    let mut server = Connection::new(server, 7);
    let frame = Credit::new(3)
        .frame(
            Header::new(7, Command::Credit)
                .with_stream(9)
                .with_flags(FLAG_LAST),
        )
        .unwrap();
    // A frame whose chunk is corrupted: its header is sound, its body is not.
    let mut corrupt = ultragraphdb_protocol_wire::ErrorReply::new(
        ultragraphdb_protocol_wire::ErrorCode::Busy,
        "busy",
    )
    .frame(Header::new(7, Command::Error).with_stream(2))
    .unwrap();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    ready(async {
        client.write_frame(frame.clone()).await.unwrap();
        client.write_frame(corrupt).await.unwrap();
    });
    let received = ready(server.read_frame(&pool)).unwrap();
    assert_eq!(received.command(), 14);
    assert_eq!(received.stream(), 9);
    assert_eq!(received.flags(), FLAG_LAST);
    assert_eq!(received.view().unwrap().command(), Command::Credit);
    let damaged = ready(server.read_frame(&pool)).unwrap();
    assert_eq!(damaged.command(), 16);
    assert!(matches!(
        damaged.view().unwrap_err().0,
        ProtocolErrorEnum::Chunk { index: 0, .. }
    ));
}
