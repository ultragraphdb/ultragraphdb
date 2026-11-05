/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The golden frames of the role commands, `testdata/golden/roles/`, written by `generate_roles.py` from the layout
//! tables of `PROTOCOL.md` §13, independently of the crate, with the values it printed; and the rebuild of a role
//! command's frame through its view, which `golden_bodies::view_any` dispatches to.

use std::fs;

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    BindView, ChallengeView, ComputeReadView, RecallView, ScanView, VerifyView,
};
use ultragraphdb_protocol_wire::{
    CatalogView, Command, EpochCloseView, FrameView, JoinView, LeaseView, MembersView,
    PlacementView, SubscribeView,
};
use ultragraphdb_protocol_wire::{
    CloseRequestView, ClosedView, DefaultEntryView, EntriesView, ReferenceView, ReportView,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_bodies::{GoldenBody, through};

/// The golden role frames, one per row: the header fields `generate_roles.py` built each with, and the size and
/// checksums it printed.
#[rustfmt::skip]
pub const ROLES: &[GoldenBody] = &[
    // 512–518
    GoldenBody { name: "join_storage", command: 512, stream: 3, flags: 0, sequence: 1, size: 308, checksum: 0x8684_c799_d4f6_8bd0, checksum_body: 0x2bdd_cd0a_960e_3521 },
    GoldenBody { name: "join_compute", command: 512, stream: 3, flags: 0, sequence: 1, size: 304, checksum: 0x03a0_e5b9_7d47_814f, checksum_body: 0x5cbc_f76f_8b64_45de },
    GoldenBody { name: "members", command: 513, stream: 5, flags: 0, sequence: 1, size: 280, checksum: 0x1457_8c29_2192_510c, checksum_body: 0xde4e_5dae_0947_2e69 },
    GoldenBody { name: "members_reply", command: 513, stream: 5, flags: 0, sequence: 1, size: 440, checksum: 0x1ffe_e2e2_0292_aed2, checksum_body: 0xf4f8_af52_1186_cd6f },
    GoldenBody { name: "subscribe", command: 514, stream: 7, flags: 0, sequence: 1, size: 256, checksum: 0xc8bc_6172_f58a_35ba, checksum_body: 0x2d06_8005_38d3_94c2 },
    GoldenBody { name: "epoch_close", command: 515, stream: 7, flags: 0, sequence: 1, size: 352, checksum: 0xa5d5_7233_7be5_3715, checksum_body: 0x8498_d876_3597_e212 },
    GoldenBody { name: "lease", command: 516, stream: 9, flags: 0, sequence: 1, size: 256, checksum: 0xf651_8249_4ba0_6475, checksum_body: 0x2d06_8005_38d3_94c2 },
    GoldenBody { name: "lease_reply", command: 516, stream: 9, flags: 0, sequence: 1, size: 256, checksum: 0x5feb_7207_a41d_e552, checksum_body: 0x2d06_8005_38d3_94c2 },
    GoldenBody { name: "placement", command: 517, stream: 11, flags: 0, sequence: 1, size: 280, checksum: 0x09d7_3eed_f47a_d2c4, checksum_body: 0xde4e_5dae_0947_2e69 },
    GoldenBody { name: "placement_reply", command: 517, stream: 11, flags: 0, sequence: 1, size: 376, checksum: 0xa48f_2d0d_b5f6_7a99, checksum_body: 0x529d_4232_247e_8efb },
    GoldenBody { name: "catalog", command: 518, stream: 13, flags: 0, sequence: 1, size: 264, checksum: 0xd0ad_d521_32e1_8413, checksum_body: 0xa1fb_3d15_0676_dcfe },
    GoldenBody { name: "catalog_reply", command: 518, stream: 13, flags: 0, sequence: 1, size: 301, checksum: 0x062a_41f1_0b92_61df, checksum_body: 0xbd89_6061_8c3f_200a },
    // 519–524
    GoldenBody { name: "challenge", command: 519, stream: 21, flags: 0, sequence: 1, size: 280, checksum: 0x64e0_d6a1_7e76_ab4a, checksum_body: 0xf293_89e0_818f_0830 },
    GoldenBody { name: "challenge_reply", command: 519, stream: 21, flags: 0, sequence: 1, size: 296, checksum: 0xf70e_9e03_98f7_6a53, checksum_body: 0x0e00_c589_a025_dfe0 },
    GoldenBody { name: "verify", command: 520, stream: 23, flags: 0, sequence: 1, size: 504, checksum: 0x6f00_7814_6985_d55e, checksum_body: 0x4979_e980_6905_a06b },
    GoldenBody { name: "verify_reply", command: 520, stream: 23, flags: 0, sequence: 1, size: 536, checksum: 0xc945_69f2_c2e6_fb87, checksum_body: 0x2c8d_7a4d_9fd3_d055 },
    GoldenBody { name: "bind", command: 521, stream: 25, flags: 0, sequence: 1, size: 256, checksum: 0x0585_fc41_7c1c_6db2, checksum_body: 0x2d06_8005_38d3_94c2 },
    GoldenBody { name: "bind_reply", command: 521, stream: 25, flags: 0, sequence: 1, size: 256, checksum: 0xcc3d_b96e_1894_1063, checksum_body: 0x2d06_8005_38d3_94c2 },
    GoldenBody { name: "bind_drop", command: 521, stream: 27, flags: 0, sequence: 1, size: 256, checksum: 0x14c4_2984_e4bd_1f7c, checksum_body: 0x2d06_8005_38d3_94c2 },
    GoldenBody { name: "recall", command: 522, stream: 29, flags: 0, sequence: 1, size: 256, checksum: 0xea53_0560_8cb4_ae1c, checksum_body: 0x2d06_8005_38d3_94c2 },
    GoldenBody { name: "compute_read", command: 523, stream: 31, flags: 0, sequence: 1, size: 411, checksum: 0x3c93_c9ec_a9a7_2a2b, checksum_body: 0x1bc2_c9b8_8c66_c5da },
    GoldenBody { name: "compute_read_rw", command: 523, stream: 33, flags: 0, sequence: 1, size: 448, checksum: 0x2f89_0a6a_c7ec_7e35, checksum_body: 0xdf8a_a61c_bc3e_bd74 },
    GoldenBody { name: "scan", command: 524, stream: 35, flags: 0, sequence: 1, size: 352, checksum: 0xf7d3_6d2f_ce8a_1895, checksum_body: 0x1d13_9a1f_84ac_9299 },
    GoldenBody { name: "scan_transaction", command: 524, stream: 37, flags: 0, sequence: 1, size: 328, checksum: 0xcd8b_0c82_459d_f67e, checksum_body: 0x3298_b6ed_95c9_3831 },
    GoldenBody { name: "scan_transaction_terms", command: 524, stream: 39, flags: 0, sequence: 1, size: 662, checksum: 0x41bb_b61e_d237_e5f1, checksum_body: 0xdb72_d7b0_ae0a_32fd },
    // 525–530
    GoldenBody { name: "entries", command: 525, stream: 23, flags: 1, sequence: 1, size: 320, checksum: 0xf14d_18ee_c2d2_25b7, checksum_body: 0x4a84_7334_1c38_740e },
    GoldenBody { name: "entries_continues", command: 525, stream: 41, flags: 0, sequence: 1, size: 376, checksum: 0x6a08_87d8_45c5_8e79, checksum_body: 0xbd5f_35bf_4f61_a812 },
    GoldenBody { name: "entries_continued", command: 525, stream: 41, flags: 1, sequence: 2, size: 336, checksum: 0xbd08_1347_0b4f_6e21, checksum_body: 0x7b22_e7ca_14b9_76ee },
    GoldenBody { name: "reference", command: 526, stream: 25, flags: 0, sequence: 1, size: 583, checksum: 0xbc03_5dbb_9e5d_174d, checksum_body: 0x408e_102e_872f_b824 },
    GoldenBody { name: "reference_reply", command: 526, stream: 25, flags: 0, sequence: 1, size: 583, checksum: 0xa963_6282_5603_b62c, checksum_body: 0xd4b8_121b_8373_3d21 },
    GoldenBody { name: "close_request", command: 527, stream: 27, flags: 1, sequence: 1, size: 256, checksum: 0x997f_7e12_6cbb_af4d, checksum_body: 0x2d06_8005_38d3_94c2 },
    GoldenBody { name: "closed", command: 528, stream: 29, flags: 0, sequence: 1, size: 312, checksum: 0xf30e_3512_8795_a05e, checksum_body: 0x8399_8b43_ae72_d26d },
    GoldenBody { name: "closed_reply", command: 528, stream: 29, flags: 0, sequence: 1, size: 312, checksum: 0x21f0_d556_5d02_805c, checksum_body: 0x6c63_b0f1_3565_3208 },
    GoldenBody { name: "default_entry", command: 529, stream: 31, flags: 0, sequence: 1, size: 313, checksum: 0xc322_f81b_17a1_34d0, checksum_body: 0xc736_9db0_2721_e061 },
    GoldenBody { name: "default_entry_reply", command: 529, stream: 31, flags: 0, sequence: 1, size: 313, checksum: 0xef91_534e_8fe4_00d4, checksum_body: 0xc736_9db0_2721_e061 },
    GoldenBody { name: "report_refused", command: 530, stream: 33, flags: 0, sequence: 1, size: 380, checksum: 0xe4f8_1996_2064_7374, checksum_body: 0x6390_2d4a_b997_8fe7 },
    GoldenBody { name: "report_passed", command: 530, stream: 35, flags: 0, sequence: 1, size: 272, checksum: 0xfe00_fc84_1083_e032, checksum_body: 0x9a5b_b525_c909_6796 },
    GoldenBody { name: "report_build_failed", command: 530, stream: 37, flags: 0, sequence: 1, size: 319, checksum: 0x7e06_2788_7a08_38b0, checksum_body: 0xa43f_5368_aadf_6488 },
];

/// The golden role frame named `name`.
///
/// # Errors
///
/// When no golden role frame has the name.
pub fn golden_role(name: &str) -> TestResult<GoldenBody> {
    ROLES
        .iter()
        .find(|b| b.name == name)
        .copied()
        .context(&format!("the golden role frame {name}"))
}

/// The bytes of the golden role frame `golden`, read from `testdata/golden/roles/<name>.frame`.
///
/// # Errors
///
/// When the file cannot be read.
pub fn role_bytes(golden: &GoldenBody) -> TestResult<Vec<u8>> {
    let path = ultragraphdb_tool_bdd::testdata_dir()?
        .join("golden")
        .join("roles")
        .join(format!("{}.frame", golden.name));
    fs::read(&path).context(&format!("reading the golden role frame {}", path.display()))
}

/// The role command's frame rebuilt from its view's owned body; a command whose view this suite does not yet read is
/// refused as not served.
///
/// # Errors
///
/// The view's refusal, or the builder's.
pub fn rebuild_role(frame: &FrameView<'_>) -> Result<Vec<u8>, ProtocolError> {
    let command = frame.command();
    match command {
        Command::Join => through::<JoinView>(frame),
        Command::Members => through::<MembersView>(frame),
        Command::Subscribe => through::<SubscribeView>(frame),
        Command::EpochClose => through::<EpochCloseView>(frame),
        Command::Lease => through::<LeaseView>(frame),
        Command::Placement => through::<PlacementView>(frame),
        Command::Catalog => through::<CatalogView>(frame),
        Command::Challenge => crate::utils::golden_bodies::through::<ChallengeView>(frame),
        Command::Verify => crate::utils::golden_bodies::through::<VerifyView>(frame),
        Command::Bind => crate::utils::golden_bodies::through::<BindView>(frame),
        Command::Recall => crate::utils::golden_bodies::through::<RecallView>(frame),
        Command::ComputeRead => crate::utils::golden_bodies::through::<ComputeReadView>(frame),
        Command::Scan => crate::utils::golden_bodies::through::<ScanView>(frame),
        Command::Entries => crate::utils::golden_bodies::through::<EntriesView>(frame),
        Command::Reference => crate::utils::golden_bodies::through::<ReferenceView>(frame),
        Command::CloseRequest => crate::utils::golden_bodies::through::<CloseRequestView>(frame),
        Command::Closed => crate::utils::golden_bodies::through::<ClosedView>(frame),
        Command::DefaultEntry => crate::utils::golden_bodies::through::<DefaultEntryView>(frame),
        Command::Report => crate::utils::golden_bodies::through::<ReportView>(frame),
        Command::Hello
        | Command::Welcome
        | Command::Register
        | Command::Begin
        | Command::Commit
        | Command::Abort
        | Command::Read
        | Command::Write
        | Command::Operation
        | Command::Prepare
        | Command::Execute
        | Command::Describe
        | Command::Result
        | Command::Credit
        | Command::Cancel
        | Command::Error
        | Command::Round
        | Command::Authenticate
        | Command::Drop
        | Command::DirectWrite
        | Command::ConditionalWrite => Err(ProtocolError::wrong_command(512, command.number())),
    }
}
