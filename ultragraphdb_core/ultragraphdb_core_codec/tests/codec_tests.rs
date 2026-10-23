/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, codec/codec_test.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_codec::{
    Decoder, Encoder, SeekPos, approx_len, decode, decode_to_buffer, encode, encode_from_buffer,
    exact_len,
};
use ultragraphdb_proto_pb::pb::UidPack;

/// Go's math/rand in the tests, as SplitMix64 with a fixed seed so that a failure reproduces.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }

    fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }
}

/// The size bound Go draws from, `rand.Intn(10e6)`.
const MAX_SIZE: u64 = 10_000_000;

fn get_uids(rng: &mut Rng, size: u64) -> Vec<u64> {
    let mut last = rng.below(100);
    let mut uids = vec![last];
    for _ in 1..size {
        last += rng.below(33);
        uids.push(last);
    }
    uids
}

fn read_uvarint(buf: &[u8]) -> (u64, usize) {
    let mut x = 0u64;
    for (i, &b) in buf.iter().enumerate() {
        x |= u64::from(b & 0x7f) << (7 * i);
        if b < 0x80 {
            return (x, i + 1);
        }
    }
    panic!("truncated varint");
}

#[test]
fn test_uid_pack() {
    // Some edge case tests.
    assert!(encode(&[], 128).is_none());
    assert_eq!(0, approx_len(&UidPack::default()));
    assert_eq!(0, decode(&UidPack::default(), 0).len());

    let mut rng = Rng(1);
    for _ in 0..13 {
        let size = rng.below(MAX_SIZE);
        let expected = get_uids(&mut rng, size);
        let pack = encode(&expected, 256).expect("a non-empty list packs");
        assert_eq!(expected.len(), exact_len(&pack));
        let actual = decode(&pack, 0);
        assert_eq!(expected, actual);
    }
}

#[test]
fn test_buffer_uid_pack() {
    // Some edge case tests.
    assert!(encode(&[], 128).is_none());

    let mut buf = Vec::new();
    decode_to_buffer(&mut buf, &UidPack::default());
    assert_eq!(0, buf.len());

    let mut rng = Rng(2);
    for _ in 0..13 {
        let size = rng.below(MAX_SIZE);
        let expected = get_uids(&mut rng, size);

        let pack = encode(&expected, 256).expect("a non-empty list packs");
        assert_eq!(expected.len(), exact_len(&pack));
        let actual = decode(&pack, 0);
        assert_eq!(expected, actual);

        let mut actual_buffer = Vec::new();
        decode_to_buffer(&mut actual_buffer, &pack);
        let enc = encode_from_buffer(&actual_buffer, 256).expect("a non-empty buffer packs");
        assert_eq!(exact_len(&pack), exact_len(&enc));

        let mut prev = 0u64;
        let mut out_buf = actual_buffer.as_slice();
        let mut uids = Vec::new();
        // Read all uids in the outBuf
        while !out_buf.is_empty() {
            let (uid, n) = read_uvarint(out_buf);
            out_buf = &out_buf[n..];

            let next = prev + uid;
            prev = next;
            uids.push(next);
        }
        assert_eq!(actual, uids);
    }
}

fn stepped_pack(n: u64, step: usize, block_size: usize) -> UidPack {
    let mut enc = Encoder::new(block_size);
    for i in (0..n).step_by(step) {
        enc.add(i);
    }
    enc.done().expect("the list is not empty")
}

#[test]
fn test_seek() {
    const N: u64 = 10001;
    let pack = stepped_pack(N, 10, 10);
    let mut dec = Decoder::unpositioned(&pack);

    struct Case {
        input: u64,
        out: u64,
        whence: SeekPos,
        empty: bool,
    }
    let case = |input, out, whence| Case {
        input,
        out,
        whence,
        empty: false,
    };
    let empty = |input, whence| Case {
        input,
        out: 0,
        whence,
        empty: true,
    };
    let tests = [
        case(0, 0, SeekPos::Start),
        case(0, 0, SeekPos::Current),
        case(100, 100, SeekPos::Start),
        case(100, 110, SeekPos::Current),
        case(1000, 1000, SeekPos::Start),
        case(1000, 1010, SeekPos::Current),
        case(1999, 2000, SeekPos::Start),
        case(1999, 2000, SeekPos::Current),
        case(1101, 1110, SeekPos::Start),
        case(1101, 1110, SeekPos::Current),
        case(10000, 10000, SeekPos::Start),
        case(9999, 10000, SeekPos::Current),
        empty(N, SeekPos::Start),
        empty(N, SeekPos::Current),
        empty(u64::MAX, SeekPos::Start),
        empty(u64::MAX, SeekPos::Current),
    ];

    for tc in &tests {
        let uids = dec.seek(tc.input, tc.whence);
        if tc.empty {
            assert!(uids.is_empty(), "seek {} returned {uids:?}", tc.input);
        } else {
            assert_eq!(tc.out, uids[0], "seek {} {:?}", tc.input, tc.whence);
        }
    }

    // Go resets dec.blockIdx to 0; a new decoder starts there.
    let mut dec = Decoder::unpositioned(&pack);
    for i in (100..10000).step_by(100) {
        let uids = dec.linear_seek(i);
        assert!(uids.contains(&i), "linear seek {i} returned {uids:?}");
    }
}

#[test]
fn test_linear_seek() {
    const N: u64 = 10001;
    let pack = stepped_pack(N, 10, 10);
    let mut dec = Decoder::unpositioned(&pack);

    for i in (0..2 * N).step_by(10) {
        let uids = dec.linear_seek(i);

        if i < N {
            assert!(uids.contains(&i), "linear seek {i} returned {uids:?}");
        } else {
            assert!(!uids.contains(&i), "linear seek {i} returned {uids:?}");
        }
    }

    // blockIdx points to last block.
    for i in (0..9990).step_by(10) {
        let uids = dec.linear_seek(i);

        assert!(!uids.contains(&i), "linear seek {i} returned {uids:?}");
    }
}

#[test]
fn test_decoder() {
    const N: u64 = 10001;
    let mut expected = Vec::new();
    let mut enc = Encoder::new(10);
    for i in (3..N).step_by(3) {
        enc.add(i);
        expected.push(i);
    }
    let pack = enc.done().expect("the list is not empty");

    let mut dec = Decoder::unpositioned(&pack);
    for i in (3..N).step_by(3) {
        let uids = dec.seek(i, SeekPos::Start);
        assert_eq!(i, uids[0]);

        let uids = dec.seek(i - 1, SeekPos::Start);
        assert_eq!(i, uids[0]);

        let uids = dec.seek(i - 2, SeekPos::Start);
        assert_eq!(i, uids[0]);

        let start = (i / 3 - 1) as usize;
        let actual = decode(&pack, i);
        assert_eq!(expected[start..], actual[..]);
    }
}

#[test]
fn test_encoding() {
    let big_ints: [u64; 5] = [
        0xf000000000000000,
        0xf00f000000000000,
        0x00f00f0000000000,
        0x000f0f0000000000,
        0x0f0f0f0f00000000,
    ];

    let lengths = [0usize, 1, 2, 3, 5, 13, 18, 100, 99, 98];

    let mut rng = Rng(3);
    for &length in &lengths {
        let mut ints = vec![0u64; length];

        for item in ints.iter_mut().take(50) {
            *item = u64::from(rng.next_u32());
        }

        for item in ints.iter_mut().skip(50) {
            *item = u64::from(rng.next_u32()) + big_ints[rng.below(5) as usize];
        }

        ints.sort_unstable();

        let decoded_ints = encode(&ints, 256)
            .map(|p| decode(&p, 0))
            .unwrap_or_default();

        assert_eq!(ints, decoded_ints);
    }
}

fn new_uid_pack(data: &[u64]) -> Option<UidPack> {
    let mut encoder = Encoder::new(10);
    for &uid in data {
        encoder.add(uid);
    }
    encoder.done()
}

#[test]
fn test_copy_uid_pack() {
    // Go's CopyUidPack is a deep copy; a UidPack's Clone is one.
    let pack = new_uid_pack(&[1, 2, 3, 4, 5]).expect("the list is not empty");
    let copy = pack.clone();
    assert_eq!(decode(&pack, 0), decode(&copy, 0));
}
