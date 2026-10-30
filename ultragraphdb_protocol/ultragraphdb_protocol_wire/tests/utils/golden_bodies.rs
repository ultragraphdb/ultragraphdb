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

/// The cluster every golden frame names.
pub const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;
/// The release every golden frame carries.
pub const RELEASE: u32 = 0x0005_0000;

/// One golden frame, as `generate_bodies.py` wrote it and printed its size and checksums.
#[derive(Debug, Clone, Copy)]
pub struct GoldenBody {
    pub name: &'static str,
    pub command: u16,
    pub stream: u64,
    pub flags: u8,
    pub sequence: u128,
    pub size: u32,
    pub checksum: u64,
    pub checksum_body: u64,
}

// One golden frame per row, its arguments the generator's printed columns in order.
#[allow(clippy::too_many_arguments)]
const fn g(
    name: &'static str,
    command: u16,
    stream: u64,
    flags: u8,
    sequence: u128,
    size: u32,
    checksum: u64,
    checksum_body: u64,
) -> GoldenBody {
    GoldenBody {
        name,
        command,
        stream,
        flags,
        sequence,
        size,
        checksum,
        checksum_body,
    }
}

// The generator's output, copied: name, command, stream, flags, sequence, size, checksum, checksum_body.
#[rustfmt::skip]
pub const BODIES: [GoldenBody; 29] = [
    g("hello", 1, 0, 0, 1, 279, 0x13cb_a202_760d_70b3, 0x3354_a6ce_4aa5_d7a6),
    g("welcome", 2, 0, 0, 1, 317, 0x4561_84ae_8da6_048f, 0x7bc2_2fd5_5be2_26d0),
    g("welcome_no_auth", 2, 0, 0, 1, 296, 0xb417_f160_aa24_f995, 0xf789_a351_5d20_daef),
    g("authenticate_client_first", 18, 0, 0, 2, 320, 0x3e70_5c2c_9545_6070, 0xea43_1ed9_5666_34a9),
    g("authenticate_server_first", 18, 0, 0, 2, 358, 0x5959_a508_58a8_c174, 0x64c6_adf9_e815_8a07),
    g("authenticate_client_final", 18, 0, 0, 3, 378, 0x001e_a553_4a66_ad71, 0xa712_172f_ecff_5b46),
    g("authenticate_server_final", 18, 0, 0, 3, 318, 0x3773_4e3a_a087_2a3c, 0x28b7_b669_06df_e983),
    g("register", 3, 0, 0, 1, 256, 0x9df1_4939_26e5_6dd6, 0x2d06_8005_38d3_94c2),
    g("begin", 4, 5, 0, 1, 256, 0xfd25_1229_2899_0449, 0x2d06_8005_38d3_94c2),
    g("commit", 5, 5, 0, 1, 296, 0x369e_f720_2bb0_49a6, 0x0a21_f801_015a_ba74),
    g("abort", 6, 5, 0, 1, 256, 0xbb90_ba9a_6b33_92ef, 0x2d06_8005_38d3_94c2),
    g("read", 7, 11, 0, 1, 411, 0xd530_96eb_a6c9_9b26, 0xd495_36ce_eedd_927f),
    g("write", 8, 5, 0, 1, 1_769, 0x10ea_04f3_2c09_f6ce, 0x1a97_c571_66f1_f5e1),
    g("operation", 9, 5, 0, 1, 541, 0x819c_9678_0d07_b06d, 0xc763_0be0_93aa_4f02),
    g("operation_counter", 9, 7, 0, 1, 387, 0x853a_2deb_e648_b96d, 0x95e0_a48a_8ed2_701c),
    g("operation_deep", 9, 9, 0, 1, 1_142, 0x08b8_0462_4235_b2ce, 0xb338_de68_2070_1115),
    g("prepare", 10, 13, 0, 1, 384, 0xc36e_3c3e_21de_e50a, 0xb7d8_3d8f_c6ef_0110),
    g("prepare_reply", 10, 13, 0, 1, 385, 0x08fb_03f7_3f5f_a1cf, 0x7237_dbcf_ca74_635e),
    g("execute", 11, 15, 0, 1, 336, 0xf929_6433_8595_88a7, 0xb518_2367_cd44_9c6f),
    g("describe", 12, 17, 0, 1, 312, 0xe91a_4ff9_f41a_ff36, 0x2464_7a33_d027_0e23),
    g("describe_reply", 12, 17, 0, 1, 508, 0x62ff_8ac6_af26_c4dc, 0x739b_5e15_2585_486c),
    g("result", 13, 11, 1, 1, 600, 0x4d6e_08e4_6c9e_c9d2, 0x59bb_9f05_6f37_4d5a),
    g("credit", 14, 11, 0, 1, 256, 0x2f73_37ee_fe0c_7c44, 0x2d06_8005_38d3_94c2),
    g("cancel", 15, 11, 0, 1, 256, 0x5032_665c_3cbb_1b5f, 0x2d06_8005_38d3_94c2),
    g("error", 16, 5, 1, 1, 312, 0xd3ee_ce7e_9292_95f8, 0x5167_54f7_3689_214d),
    g("error_operation", 16, 7, 1, 1, 348, 0x2e88_d89f_9888_6cf3, 0x2897_5799_bf0a_e6df),
    g("drop", 19, 19, 0, 1, 311, 0x6d0b_29f7_340a_0d3b, 0xabf6_85b8_5add_94f2),
    g("drop_reply", 19, 19, 1, 1, 311, 0xeade_41ee_16c1_aee0, 0xabf6_85b8_5add_94f2),
    g("round", 17, 0, 0, 3, 5_792, 0x8448_5601_23ef_5d44, 0x0913_d3cc_f34d_1ed6),
];

/// The schema id the generator printed for the golden Describe answer's declarations.
pub const DESCRIBE_SCHEMA_ID: u64 = 0xfe70_341d_112b_65a9;

/// The golden frame named `name`.
///
/// # Panics
///
/// When no golden frame has the name.
pub fn golden(name: &str) -> GoldenBody {
    *BODIES
        .iter()
        .find(|b| b.name == name)
        .unwrap_or_else(|| panic!("no golden frame {name}"))
}

impl GoldenBody {
    /// The frame's bytes, read from `testdata/golden/bodies/<name>.frame`.
    ///
    /// # Panics
    ///
    /// When the file cannot be read.
    pub fn bytes(&self) -> Vec<u8> {
        let path = ultragraphdb_tool_bdd::testdata_dir()
            .join("golden")
            .join("bodies")
            .join(format!("{}.frame", self.name));
        fs::read(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
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
    pub fn data(&self) -> Vec<u8> {
        body_data(&self.bytes()).to_vec()
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
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

/// `(offset, len)` of section `index` of a body whose fixed part is `fixed` bytes.
pub fn section(data: &[u8], fixed: usize, index: usize) -> (usize, usize) {
    let at = fixed + 8 * index;
    (u32_at(data, at) as usize, u32_at(data, at + 4) as usize)
}

/// `bytes` validated and read through its command's view, and the view read back to its owned body, which builds the
/// same frame again under the frame's own header: byte for byte, or, for Write, Describe and Result, whose names and
/// result terms a builder interns, a frame that rebuilds to itself.
pub fn view_any(bytes: &[u8]) -> Result<(), ProtocolError> {
    let (command, rebuilt) = rebuild(bytes)?;
    if matches!(
        command,
        Command::Write | Command::Describe | Command::Result
    ) {
        let (_, again) = rebuild(&rebuilt).expect("a rebuilt frame reads");
        assert_eq!(again, rebuilt, "{command:?} is not stable");
    } else {
        assert_eq!(rebuilt, bytes, "{command:?} rebuilt differs");
    }
    Ok(())
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
/// # Panics
///
/// When the frame does not validate.
pub fn frame_of(bytes: &[u8]) -> FrameView<'_> {
    FrameView::validate(bytes, CLUSTER, MAX_FRAME_SIZE).expect("the frame validates")
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
