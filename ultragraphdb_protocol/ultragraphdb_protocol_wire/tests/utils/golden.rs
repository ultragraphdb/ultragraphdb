/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The golden frames of `testdata/golden/`, written by `generate_frames.c` with the xxHash reference implementation
//! (v0.8.3), and the values it printed for each; shared by the unit tests and, by path, the BDD target.

use std::fs;

/// The cluster every golden frame names.
pub const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

/// One golden frame's fields, as `generate_frames.c` wrote them and printed its checksums.
pub struct GoldenFrame {
    pub name: &'static str,
    pub size: u32,
    pub checksum: u64,
    pub checksum_body: u64,
    pub view: u32,
    pub release: u32,
    pub protocol: u16,
    pub command: u16,
    pub stream: u64,
    pub replica: u8,
    pub flags: u8,
    /// Command header byte i is `(i * 7 + 3) ^ command_header_xor`.
    pub command_header_xor: u8,
    pub data_len: usize,
    /// The chunk table's leaves; empty for a body of one chunk.
    pub leaves: &'static [u64],
}

pub const HELLO: GoldenFrame = GoldenFrame {
    name: "hello",
    size: 356,
    checksum: 0x8B60_6B87_756C_942F,
    checksum_body: 0x93CD_9543_2B7D_483F,
    view: 0,
    release: 0x0005_0000,
    protocol: 1,
    command: 1,
    stream: 0,
    replica: 0,
    flags: 0,
    command_header_xor: 0x00,
    data_len: 100,
    leaves: &[],
};

pub const READ: GoldenFrame = GoldenFrame {
    name: "read",
    size: 20_736,
    checksum: 0xBDCE_FE15_6191_9967,
    checksum_body: 0xB5B8_0037_54D2_87ED,
    view: 0x0A0B_0C0D,
    release: 0x0102_0304,
    protocol: 0x0302,
    command: 7,
    stream: 0x1122_3344_5566_7788,
    replica: 0x5A,
    flags: 0xA5,
    command_header_xor: 0xFF,
    data_len: 20_432,
    leaves: &[
        0x37AA_5612_B319_22B4,
        0x40AA_D37E_3522_AED0,
        0x2A37_1389_5DB3_B32D,
        0x576B_E5FF_719C_59BC,
        0x729D_B17E_0268_1F96,
        0xC45B_8654_3EFC_5433,
    ],
};

pub const CREDIT: GoldenFrame = GoldenFrame {
    name: "credit",
    size: 256,
    checksum: 0x3229_CB0A_4A9D_D243,
    // XXH3-64 of no bytes with seed 0: XSUM_XXH3_testdata[0] of xxHash's sanity vectors as well.
    checksum_body: 0x2D06_8005_38D3_94C2,
    view: 0,
    release: 0x0005_0000,
    protocol: 1,
    command: 14,
    stream: 9,
    replica: 0,
    flags: 1,
    command_header_xor: 0x11,
    data_len: 0,
    leaves: &[],
};

pub const ONE_CHUNK: GoldenFrame = GoldenFrame {
    name: "one_chunk",
    size: 4_096,
    checksum: 0xCDA9_05CC_BD1D_6CB8,
    checksum_body: 0xF1BB_CE96_A4C6_4EC0,
    view: 0,
    release: 0x0005_0000,
    protocol: 1,
    command: 13,
    stream: 3,
    replica: 0,
    flags: 0,
    command_header_xor: 0x22,
    data_len: 3_840,
    leaves: &[],
};

pub const TWO_CHUNKS: GoldenFrame = GoldenFrame {
    name: "two_chunks",
    size: 4_113,
    checksum: 0x4245_082D_9956_F385,
    checksum_body: 0x4169_EE9E_34DB_3D5A,
    view: 0,
    release: 0x0005_0000,
    protocol: 1,
    command: 13,
    stream: 4,
    replica: 0,
    flags: 0,
    command_header_xor: 0x33,
    data_len: 3_841,
    leaves: &[0xF58B_6858_34EC_C49D, 0xB543_05B2_405A_3A2E],
};

pub const ROUND: GoldenFrame = GoldenFrame {
    name: "round",
    size: 8_208,
    checksum: 0x31C6_314C_AE27_02C3,
    checksum_body: 0x8869_90A3_C4C5_0C16,
    view: 0,
    release: 0x0005_0000,
    protocol: 1,
    command: 17,
    stream: 0,
    replica: 0,
    flags: 0,
    command_header_xor: 0x44,
    data_len: 7_936,
    leaves: &[0xA821_56A2_7AF6_3792, 0x9B92_ABAA_4208_A46D],
};

pub const GOLDEN: [GoldenFrame; 6] = [HELLO, READ, CREDIT, ONE_CHUNK, TWO_CHUNKS, ROUND];

impl GoldenFrame {
    /// The frame's bytes, read from `testdata/golden/<name>.frame`.
    pub fn bytes(&self) -> Vec<u8> {
        let path = ultragraphdb_tool_bdd::testdata_dir()
            .join("golden")
            .join(format!("{}.frame", self.name));
        fs::read(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
    }

    /// The body's bytes before its table.
    pub fn data(&self) -> Vec<u8> {
        self.bytes()[256..256 + self.data_len].to_vec()
    }

    /// The 128-byte command header the generator wrote.
    pub fn command_header(&self) -> [u8; 128] {
        std::array::from_fn(|i| ((i * 7 + 3) as u8) ^ self.command_header_xor)
    }
}

/// Writes `value` little-endian at `at`.
pub fn put(frame: &mut [u8], at: usize, value: u64, width: usize) {
    frame[at..at + width].copy_from_slice(&value.to_le_bytes()[..width]);
}

/// Recomputes the header checksum after a test changed a field, with xxhash-rust directly rather than through the
/// crate under test, so a rule after the checksum is reached.
pub fn reseal(frame: &mut [u8]) {
    let checksum = xxhash_rust::xxh3::xxh3_64(&frame[8..256]);
    put(frame, 0, checksum, 8);
}

/// The first `len` bytes of xxHash's sanity buffer, filled as tests/sanity_test.c's fillTestBuffer fills it.
pub fn sanity_buffer(len: usize) -> Vec<u8> {
    let mut byte_gen: u64 = 2_654_435_761;
    (0..len)
        .map(|_| {
            let byte = (byte_gen >> 56) as u8;
            byte_gen = byte_gen.wrapping_mul(11_400_714_785_074_694_797);
            byte
        })
        .collect()
}
