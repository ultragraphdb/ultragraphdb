/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{Command, Header};

#[test]
fn test_new_header_is_zero_but_its_cluster_command_and_protocol() {
    let header = Header::new(0x0123_4567_89AB_CDEF, Command::Read);
    assert_eq!(header.cluster(), 0x0123_4567_89AB_CDEF);
    assert_eq!(header.command(), Command::Read);
    assert_eq!(header.protocol(), 1);
    assert_eq!(header.view(), 0);
    assert_eq!(header.release(), 0);
    assert_eq!(header.stream(), 0);
    assert_eq!(header.replica(), 0);
    assert_eq!(header.flags(), 0);
    assert_eq!(header.command_header(), &[0u8; 128]);
}

#[test]
fn test_each_setter_sets_its_field_alone() {
    let mut command_header = [0u8; 128];
    command_header[0] = 0xC0;
    command_header[127] = 0xDE;
    // Distinct values per field, so a setter writing another field's slot fails.
    let header = Header::new(7, Command::Commit)
        .with_view(0x0A0B_0C0D)
        .with_release(0x0102_0304)
        .with_protocol(0x0302)
        .with_stream(0x1122_3344_5566_7788)
        .with_replica(0x5A)
        .with_flags(0xA5)
        .with_command_header(command_header);
    assert_eq!(header.cluster(), 7);
    assert_eq!(header.command(), Command::Commit);
    assert_eq!(header.view(), 0x0A0B_0C0D);
    assert_eq!(header.release(), 0x0102_0304);
    assert_eq!(header.protocol(), 0x0302);
    assert_eq!(header.stream(), 0x1122_3344_5566_7788);
    assert_eq!(header.replica(), 0x5A);
    assert_eq!(header.flags(), 0xA5);
    assert_eq!(header.command_header(), &command_header);

    let only_view = Header::new(7, Command::Commit).with_view(1);
    assert_eq!(
        (only_view.release(), only_view.stream(), only_view.flags()),
        (0, 0, 0)
    );
    assert_eq!(only_view.protocol(), 1);
}

#[test]
fn test_headers_compare_by_every_field() {
    let a = Header::new(1, Command::Hello);
    assert_eq!(a.clone(), Header::new(1, Command::Hello));
    assert_ne!(a.clone(), Header::new(2, Command::Hello));
    assert_ne!(a.clone(), Header::new(1, Command::Welcome));
    assert_ne!(a.clone(), a.clone().with_flags(1));
}
