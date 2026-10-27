/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{
    CHUNK_SIZE, COMMAND_HEADER_SIZE, Command, FIRST_CHUNK_SIZE, HEADER_SIZE, LEAF_SIZE,
    MAX_FRAME_SIZE, PROTOCOL_VERSION, REPLICA_COMMANDS, RETIRED_COMMANDS,
};

#[test]
fn test_sizes_are_the_specs() {
    // The spec's header table: 256 bytes, its command header the last 128.
    assert_eq!(HEADER_SIZE, 256);
    assert_eq!(COMMAND_HEADER_SIZE, 128);
    // 1 MiB.
    assert_eq!(MAX_FRAME_SIZE, 1_048_576);
    // A frame's first 4,096-byte block holds the header and the first chunk.
    assert_eq!(FIRST_CHUNK_SIZE, 3_840);
    assert_eq!(FIRST_CHUNK_SIZE + HEADER_SIZE, CHUNK_SIZE);
    assert_eq!(CHUNK_SIZE, 4_096);
    assert_eq!(MAX_FRAME_SIZE % CHUNK_SIZE, 0);
    assert_eq!(LEAF_SIZE, 8);
    assert_eq!(PROTOCOL_VERSION, 1);
}

#[test]
fn test_reserved_numbers_belong_to_no_command() {
    assert_eq!(REPLICA_COMMANDS, 256..=511);
    for command in Command::ALL {
        assert!(!REPLICA_COMMANDS.contains(&command.number()), "{command:?}");
        assert!(!RETIRED_COMMANDS.contains(&command.number()), "{command:?}");
    }
    for retired in RETIRED_COMMANDS {
        assert!(!REPLICA_COMMANDS.contains(retired), "{retired}");
    }
    // No command has been retired yet.
    assert!(RETIRED_COMMANDS.is_empty());
}
