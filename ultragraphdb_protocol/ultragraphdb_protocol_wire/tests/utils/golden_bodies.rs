/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The golden frames of every command's body, `testdata/golden/bodies/`, written by `generate_bodies.py` from the
//! layout tables, independently of the crate, with the values it printed; and the helpers that rebuild a frame around
//! a changed body or command header with xxhash-rust directly, so a mutated body reaches the body's checks.

use std::fs;

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    AbortView, AuthenticateView, BeginView, BodyBuilder, BodyView, CancelView, Command, CommitView,
    CreditView, DescribeView, DropCommandView, ErrorReplyView, ExecuteView, FrameView, Header,
    HelloView, MAX_FRAME_SIZE, OperationView, PrepareView, ReadView, RegisterView, ResultView,
    RoundView, WelcomeView, WriteView,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// The cluster every golden frame names.
pub const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;
/// The release every golden frame carries.
pub const RELEASE: u32 = 0x0005_0000;

/// One golden frame, as `generate_bodies.py` wrote it and printed its size and checksums.
#[derive(Debug, Clone, Copy)]
pub struct GoldenBody {
    /// The frame's name, the stem of its file under `testdata/golden/bodies/`.
    pub name: &'static str,
    /// The header's command code.
    pub command: u16,
    /// The header's stream.
    pub stream: u64,
    /// The header's flags.
    pub flags: u8,
    /// The header's sequence.
    pub sequence: u128,
    /// The frame's size in bytes.
    pub size: u32,
    /// The header's checksum: XXH3-64 of the header after its first 8 bytes.
    pub checksum: u64,
    /// The body's checksum: XXH3-64 of the body when it is one chunk, of its chunk table otherwise.
    pub checksum_body: u64,
}

/// The golden frames, one per row: the header fields `generate_bodies.py` built each with, and the size and
/// checksums it printed.
#[rustfmt::skip]
pub const BODIES: [GoldenBody; 29] = [
    GoldenBody { name: "hello", command: 1, stream: 0, flags: 0, sequence: 1, size: 279, checksum: 0x13cb_a202_760d_70b3, checksum_body: 0x3354_a6ce_4aa5_d7a6 },
    GoldenBody { name: "welcome", command: 2, stream: 0, flags: 0, sequence: 1, size: 317, checksum: 0x4561_84ae_8da6_048f, checksum_body: 0x7bc2_2fd5_5be2_26d0 },
    GoldenBody { name: "welcome_no_auth", command: 2, stream: 0, flags: 0, sequence: 1, size: 296, checksum: 0xb417_f160_aa24_f995, checksum_body: 0xf789_a351_5d20_daef },
    GoldenBody { name: "authenticate_client_first", command: 18, stream: 0, flags: 0, sequence: 2, size: 320, checksum: 0x3e70_5c2c_9545_6070, checksum_body: 0xea43_1ed9_5666_34a9 },
    GoldenBody { name: "authenticate_server_first", command: 18, stream: 0, flags: 0, sequence: 2, size: 358, checksum: 0x5959_a508_58a8_c174, checksum_body: 0x64c6_adf9_e815_8a07 },
    GoldenBody { name: "authenticate_client_final", command: 18, stream: 0, flags: 0, sequence: 3, size: 378, checksum: 0x001e_a553_4a66_ad71, checksum_body: 0xa712_172f_ecff_5b46 },
    GoldenBody { name: "authenticate_server_final", command: 18, stream: 0, flags: 0, sequence: 3, size: 318, checksum: 0x3773_4e3a_a087_2a3c, checksum_body: 0x28b7_b669_06df_e983 },
    GoldenBody { name: "register", command: 3, stream: 0, flags: 0, sequence: 1, size: 256, checksum: 0x9df1_4939_26e5_6dd6, checksum_body: 0x2d06_8005_38d3_94c2 },
    GoldenBody { name: "begin", command: 4, stream: 5, flags: 0, sequence: 1, size: 256, checksum: 0xfd25_1229_2899_0449, checksum_body: 0x2d06_8005_38d3_94c2 },
    GoldenBody { name: "commit", command: 5, stream: 5, flags: 0, sequence: 1, size: 296, checksum: 0xa273_55ad_e16c_06e6, checksum_body: 0xff71_5c73_e704_ed19 },
    GoldenBody { name: "abort", command: 6, stream: 5, flags: 0, sequence: 1, size: 256, checksum: 0xbb90_ba9a_6b33_92ef, checksum_body: 0x2d06_8005_38d3_94c2 },
    GoldenBody { name: "read", command: 7, stream: 11, flags: 0, sequence: 1, size: 411, checksum: 0xd530_96eb_a6c9_9b26, checksum_body: 0xd495_36ce_eedd_927f },
    GoldenBody { name: "write", command: 8, stream: 5, flags: 0, sequence: 1, size: 1_769, checksum: 0x10ea_04f3_2c09_f6ce, checksum_body: 0x1a97_c571_66f1_f5e1 },
    GoldenBody { name: "operation", command: 9, stream: 5, flags: 0, sequence: 1, size: 541, checksum: 0x819c_9678_0d07_b06d, checksum_body: 0xc763_0be0_93aa_4f02 },
    GoldenBody { name: "operation_counter", command: 9, stream: 7, flags: 0, sequence: 1, size: 387, checksum: 0x853a_2deb_e648_b96d, checksum_body: 0x95e0_a48a_8ed2_701c },
    GoldenBody { name: "operation_deep", command: 9, stream: 9, flags: 0, sequence: 1, size: 1_142, checksum: 0x08b8_0462_4235_b2ce, checksum_body: 0xb338_de68_2070_1115 },
    GoldenBody { name: "prepare", command: 10, stream: 13, flags: 0, sequence: 1, size: 384, checksum: 0xc36e_3c3e_21de_e50a, checksum_body: 0xb7d8_3d8f_c6ef_0110 },
    GoldenBody { name: "prepare_reply", command: 10, stream: 13, flags: 0, sequence: 1, size: 385, checksum: 0x08fb_03f7_3f5f_a1cf, checksum_body: 0x7237_dbcf_ca74_635e },
    GoldenBody { name: "execute", command: 11, stream: 15, flags: 0, sequence: 1, size: 336, checksum: 0xf929_6433_8595_88a7, checksum_body: 0xb518_2367_cd44_9c6f },
    GoldenBody { name: "describe", command: 12, stream: 17, flags: 0, sequence: 1, size: 312, checksum: 0xe91a_4ff9_f41a_ff36, checksum_body: 0x2464_7a33_d027_0e23 },
    GoldenBody { name: "describe_reply", command: 12, stream: 17, flags: 0, sequence: 1, size: 508, checksum: 0x62ff_8ac6_af26_c4dc, checksum_body: 0x739b_5e15_2585_486c },
    GoldenBody { name: "result", command: 13, stream: 11, flags: 1, sequence: 1, size: 648, checksum: 0x87f7_d5e3_4b9a_e087, checksum_body: 0xaeea_d453_d05a_0ed8 },
    GoldenBody { name: "credit", command: 14, stream: 11, flags: 0, sequence: 1, size: 256, checksum: 0x2f73_37ee_fe0c_7c44, checksum_body: 0x2d06_8005_38d3_94c2 },
    GoldenBody { name: "cancel", command: 15, stream: 11, flags: 0, sequence: 1, size: 256, checksum: 0x5032_665c_3cbb_1b5f, checksum_body: 0x2d06_8005_38d3_94c2 },
    GoldenBody { name: "error", command: 16, stream: 5, flags: 1, sequence: 1, size: 312, checksum: 0xd3ee_ce7e_9292_95f8, checksum_body: 0x5167_54f7_3689_214d },
    GoldenBody { name: "error_operation", command: 16, stream: 7, flags: 1, sequence: 1, size: 348, checksum: 0x2e88_d89f_9888_6cf3, checksum_body: 0x2897_5799_bf0a_e6df },
    GoldenBody { name: "drop", command: 19, stream: 19, flags: 0, sequence: 1, size: 311, checksum: 0x6d0b_29f7_340a_0d3b, checksum_body: 0xabf6_85b8_5add_94f2 },
    GoldenBody { name: "drop_reply", command: 19, stream: 19, flags: 1, sequence: 1, size: 311, checksum: 0xeade_41ee_16c1_aee0, checksum_body: 0xabf6_85b8_5add_94f2 },
    GoldenBody { name: "round", command: 17, stream: 0, flags: 0, sequence: 3, size: 5_520, checksum: 0x9516_930d_4ee7_72f5, checksum_body: 0xf685_3c79_e1db_4579 },
];

/// The schema id the generator printed for the golden Describe answer's declarations.
pub const DESCRIBE_SCHEMA_ID: u64 = 0xfe70_341d_112b_65a9;

/// The golden frame named `name`.
///
/// # Errors
///
/// When no golden frame has the name.
pub fn golden(name: &str) -> TestResult<GoldenBody> {
    BODIES
        .iter()
        .find(|b| b.name == name)
        .copied()
        .context(&format!("the golden frame {name}"))
}

impl GoldenBody {
    /// The frame's bytes, read from `testdata/golden/bodies/<name>.frame`.
    ///
    /// # Errors
    ///
    /// When the file cannot be read.
    pub fn bytes(&self) -> TestResult<Vec<u8>> {
        let path = ultragraphdb_tool_bdd::testdata_dir()?
            .join("golden")
            .join("bodies")
            .join(format!("{}.frame", self.name));
        fs::read(&path).context(&format!("reading the golden frame {}", path.display()))
    }

    /// The header a builder is given to rebuild the frame: the golden fields but the command and command header,
    /// which the builder supplies.
    pub fn header(&self) -> Header {
        Header::new(CLUSTER, Command::Abort)
            .with_sequence(self.sequence)
            .with_release(RELEASE)
            .with_stream(self.stream)
            .with_flags(self.flags)
    }

    /// The body's bytes before its chunk table, split by the oracle below.
    ///
    /// # Errors
    ///
    /// When the file cannot be read.
    pub fn data(&self) -> TestResult<Vec<u8>> {
        Ok(body_data(&self.bytes()?).to_vec())
    }
}

/// The bytes the chunks of a frame's body cover: the body without its table, the length found by counting chunks of
/// 3,840 and then 4,096 bytes until data and table add up, independently of the crate's closed form.
pub fn body_data(frame: &[u8]) -> &[u8] {
    let body = &frame[256..];
    if body.len() <= 3_840 {
        return body;
    }
    let mut chunks = 2;
    loop {
        let data = body.len() - 8 * chunks;
        let needed = 1 + (data - 3_840).div_ceil(4_096);
        if needed == chunks {
            return &body[..data];
        }
        chunks += 1;
    }
}

/// `frame`'s header around `data`, the body's new bytes: `size`, the chunk table, `checksum_body` and `checksum`
/// recomputed with xxhash-rust, so the frame passes every check before the body's structure.
pub fn reframe(frame: &[u8], data: &[u8]) -> Vec<u8> {
    use xxhash_rust::xxh3::{xxh3_64, xxh3_64_with_seed};
    let mut chunks = vec![&data[..data.len().min(3_840)]];
    let mut at = 3_840;
    while at < data.len() {
        chunks.push(&data[at..(at + 4_096).min(data.len())]);
        at += 4_096;
    }
    let leaves: Vec<u64> = chunks
        .iter()
        .enumerate()
        .map(|(i, c)| xxh3_64_with_seed(c, i as u64))
        .collect();
    let (table, checksum_body) = if leaves.len() == 1 {
        (Vec::new(), leaves[0])
    } else {
        let table: Vec<u8> = leaves.iter().flat_map(|l| l.to_le_bytes()).collect();
        let root = xxh3_64(&table);
        (table, root)
    };
    let mut out = frame[..256].to_vec();
    out.extend_from_slice(data);
    out.extend_from_slice(&table);
    let size = out.len() as u32;
    out[40..44].copy_from_slice(&size.to_le_bytes());
    out[8..16].copy_from_slice(&checksum_body.to_le_bytes());
    let checksum = xxh3_64(&out[8..256]);
    out[0..8].copy_from_slice(&checksum.to_le_bytes());
    out
}

/// `frame` with its command header changed by `change` and the header checksum recomputed with xxhash-rust.
pub fn with_command_header(frame: &[u8], change: impl FnOnce(&mut [u8])) -> Vec<u8> {
    let mut out = frame.to_vec();
    change(&mut out[128..256]);
    let checksum = xxhash_rust::xxh3::xxh3_64(&out[8..256]);
    out[0..8].copy_from_slice(&checksum.to_le_bytes());
    out
}

/// `frame` with its header's `command` set to `command` and the checksum recomputed.
pub fn with_command(frame: &[u8], command: u16) -> Vec<u8> {
    let mut out = frame.to_vec();
    out[54..56].copy_from_slice(&command.to_le_bytes());
    let checksum = xxhash_rust::xxh3::xxh3_64(&out[8..256]);
    out[0..8].copy_from_slice(&checksum.to_le_bytes());
    out
}

/// Writes `value` little-endian at `at`, `width` bytes.
pub fn put(bytes: &mut [u8], at: usize, value: u64, width: usize) {
    bytes[at..at + width].copy_from_slice(&value.to_le_bytes()[..width]);
}

/// The little-endian u32 at `at`.
pub fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(std::array::from_fn(|i| bytes[at + i]))
}

/// `(offset, len)` of section `index` of a body whose fixed part is `fixed` bytes.
pub fn section(data: &[u8], fixed: usize, index: usize) -> (usize, usize) {
    let at = fixed + 8 * index;
    (u32_at(data, at) as usize, u32_at(data, at + 4) as usize)
}

/// `bytes` validated and read through its command's view, and the view read back to its owned body, which builds the
/// same frame again under the frame's own header: byte for byte, or, for Write, Describe and Result, whose names and
/// result terms a builder interns, a frame that rebuilds to itself. The inner result is the frame's refusal; the outer
/// fails the test when a rebuilt frame does not read.
///
/// # Errors
///
/// When a rebuilt frame does not read again.
pub fn view_any(bytes: &[u8]) -> TestResult<Result<(), ProtocolError>> {
    let (command, rebuilt) = match rebuild(bytes) {
        Ok(read) => read,
        Err(refusal) => return Ok(Err(refusal)),
    };
    if matches!(
        command,
        Command::Write | Command::Describe | Command::Result
    ) {
        let (_, again) = rebuild(&rebuilt).context("a rebuilt frame reads")?;
        assert_eq!(again, rebuilt, "{command:?} is not stable");
    } else {
        assert_eq!(rebuilt, bytes, "{command:?} rebuilt differs");
    }
    Ok(Ok(()))
}

/// The frame `bytes` rebuilt from its view's owned body.
fn rebuild(bytes: &[u8]) -> Result<(Command, Vec<u8>), ProtocolError> {
    fn through<'a, V: BodyView<'a>>(frame: &FrameView<'a>) -> Result<Vec<u8>, ProtocolError> {
        let body = V::from_frame(frame)?.to_body()?;
        let h = frame.header();
        let header = Header::new(h.cluster(), Command::Abort)
            .with_sequence(h.sequence())
            .with_view(h.view())
            .with_release(h.release())
            .with_protocol(h.protocol())
            .with_stream(h.stream())
            .with_replica(h.replica())
            .with_flags(h.flags());
        body.frame(header)
    }
    let frame = FrameView::validate(bytes, CLUSTER, MAX_FRAME_SIZE)?;
    let rebuilt = match frame.command() {
        Command::Hello => through::<HelloView>(&frame),
        Command::Welcome => through::<WelcomeView>(&frame),
        Command::Register => through::<RegisterView>(&frame),
        Command::Begin => through::<BeginView>(&frame),
        Command::Commit => through::<CommitView>(&frame),
        Command::Abort => through::<AbortView>(&frame),
        Command::Read => through::<ReadView>(&frame),
        Command::Write => through::<WriteView>(&frame),
        Command::Operation => through::<OperationView>(&frame),
        Command::Prepare => through::<PrepareView>(&frame),
        Command::Execute => through::<ExecuteView>(&frame),
        Command::Describe => through::<DescribeView>(&frame),
        Command::Result => through::<ResultView>(&frame),
        Command::Credit => through::<CreditView>(&frame),
        Command::Cancel => through::<CancelView>(&frame),
        Command::Error => through::<ErrorReplyView>(&frame),
        Command::Round => through::<RoundView>(&frame),
        Command::Authenticate => through::<AuthenticateView>(&frame),
        Command::Drop => through::<DropCommandView>(&frame),
    }?;
    Ok((frame.command(), rebuilt))
}

/// `frame` validated, for a view under test.
///
/// # Errors
///
/// When the frame does not validate.
pub fn frame_of(bytes: &[u8]) -> TestResult<FrameView<'_>> {
    FrameView::validate(bytes, CLUSTER, MAX_FRAME_SIZE).context("the frame validates")
}

/// The `n` sections of `data` after a fixed part of `fixed` bytes, by its table.
pub fn split_sections(data: &[u8], fixed: usize, n: usize) -> Vec<Vec<u8>> {
    (0..n)
        .map(|i| {
            let (offset, len) = section(data, fixed, i);
            data[offset..offset + len].to_vec()
        })
        .collect()
}

/// A body of `fixed` and `secs` in the layout, laid out here from the layout table: the table of offsets and
/// lengths, each section at the end of the one before rounded up to 8, zero padding.
pub fn join_sections(fixed: &[u8], secs: &[Vec<u8>]) -> Vec<u8> {
    let mut at = fixed.len() + 8 * secs.len();
    let mut table = Vec::new();
    for s in secs {
        at = at.div_ceil(8) * 8;
        table.push((at, s.len()));
        at += s.len();
    }
    let mut out = fixed.to_vec();
    for (offset, len) in &table {
        out.extend_from_slice(&(*offset as u32).to_le_bytes());
        out.extend_from_slice(&(*len as u32).to_le_bytes());
    }
    for ((offset, _), s) in table.iter().zip(secs) {
        out.resize(*offset, 0);
        out.extend_from_slice(s);
    }
    out
}

/// `frame` with section `index` of its body (no fixed part, `n` sections) replaced by `change` of it.
pub fn with_section(
    frame: &[u8],
    n: usize,
    index: usize,
    change: impl FnOnce(&mut Vec<u8>),
) -> Vec<u8> {
    let mut secs = split_sections(body_data(frame), 0, n);
    change(&mut secs[index]);
    reframe(frame, &join_sections(&[], &secs))
}
