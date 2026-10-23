/*
 * SPDX-FileCopyrightText: © 2014-2017 Damian Gryski
 * SPDX-FileCopyrightText: © 2016-2017 Nicola Asuni - Tecnick.com
 * SPDX-License-Identifier: MIT
 *
 * Ported from github.com/dgryski/go-farm (fp_generic.go: Fingerprint64; farmhashna.go: naHash64 and the helpers it
 * calls; basics.go: k0, k1, k2) at v0.0.0-20240924180020-3414d57e47da, the version Dgraph v25.5.0 uses. Changed for
 * ultragraphdb: translated to Rust; Fingerprint64 alone, the one go-farm function Dgraph calls; words read by offset
 * into the input rather than through re-sliced input.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! go-farm's `Fingerprint64`, FarmHash's `Fingerprint64` (farmhashna): the 64-bit fingerprint Dgraph gives a
//! language-tagged or list value's posting (`posting/list.go`), and the hash of its conflict keys. A fingerprint is
//! fixed forever: it is the same on every platform and in every version, so it may be stored.

/// Primes between 2^63 and 2^64.
const K0: u64 = 0xc3a5c85c97cb3127;
const K1: u64 = 0xb492b66fbe98f273;
const K2: u64 = 0x9ae16a3b2f90404f;

/// go-farm's `Fingerprint64`: the fingerprint of `s`, as Go's `farm.Fingerprint64(s)` returns it on every
/// architecture.
pub fn fingerprint64(s: &[u8]) -> u64 {
    let slen = s.len();
    if slen <= 16 {
        return hash_len_0_to_16(s);
    }
    if slen <= 32 {
        return hash_len_17_to_32(s);
    }
    if slen <= 64 {
        return hash_len_33_to_64(s);
    }
    hash_len_65_plus(s)
}

/// The little-endian `u64` at `i`. Every caller reads within `s`, by the length branch it is called from; a read past
/// the end, which none makes, gives 0 rather than a panic.
fn fetch64(s: &[u8], i: usize) -> u64 {
    s.get(i..)
        .and_then(<[u8]>::first_chunk::<8>)
        .map_or(0, |b| u64::from_le_bytes(*b))
}

/// The little-endian `u32` at `i`, read as [`fetch64`] reads.
fn fetch32(s: &[u8], i: usize) -> u64 {
    s.get(i..)
        .and_then(<[u8]>::first_chunk::<4>)
        .map_or(0, |b| u64::from(u32::from_le_bytes(*b)))
}

/// The byte at `i`, read as [`fetch64`] reads.
fn byte(s: &[u8], i: usize) -> u32 {
    s.get(i).map_or(0, |&b| u32::from(b))
}

fn shift_mix(val: u64) -> u64 {
    val ^ (val >> 47)
}

/// Murmur-inspired hashing of two words.
fn hash_len_16_mul(u: u64, v: u64, mul: u64) -> u64 {
    let a = shift_mix((u ^ v).wrapping_mul(mul));
    let b = shift_mix((v ^ a).wrapping_mul(mul));
    b.wrapping_mul(mul)
}

fn hash_len_0_to_16(s: &[u8]) -> u64 {
    let slen = s.len();
    let len = slen as u64;
    if slen >= 8 {
        let mul = K2.wrapping_add(len.wrapping_mul(2));
        let a = fetch64(s, 0).wrapping_add(K2);
        let b = fetch64(s, slen - 8);
        let c = b.rotate_right(37).wrapping_mul(mul).wrapping_add(a);
        let d = a.rotate_right(25).wrapping_add(b).wrapping_mul(mul);
        return hash_len_16_mul(c, d, mul);
    }
    if slen >= 4 {
        let mul = K2.wrapping_add(len.wrapping_mul(2));
        let a = fetch32(s, 0);
        return hash_len_16_mul(len.wrapping_add(a << 3), fetch32(s, slen - 4), mul);
    }
    if slen > 0 {
        let a = byte(s, 0);
        let b = byte(s, slen >> 1);
        let c = byte(s, slen - 1);
        let y = a + (b << 8);
        let z = slen as u32 + (c << 2);
        return shift_mix(u64::from(y).wrapping_mul(K2) ^ u64::from(z).wrapping_mul(K0))
            .wrapping_mul(K2);
    }
    K2
}

fn hash_len_17_to_32(s: &[u8]) -> u64 {
    let slen = s.len();
    let mul = K2.wrapping_add((slen as u64).wrapping_mul(2));
    let a = fetch64(s, 0).wrapping_mul(K1);
    let b = fetch64(s, 8);
    let c = fetch64(s, slen - 8).wrapping_mul(mul);
    let d = fetch64(s, slen - 16).wrapping_mul(K2);
    hash_len_16_mul(
        a.wrapping_add(b)
            .rotate_right(43)
            .wrapping_add(c.rotate_right(30))
            .wrapping_add(d),
        a.wrapping_add(b.wrapping_add(K2).rotate_right(18))
            .wrapping_add(c),
        mul,
    )
}

/// A 16-byte hash of four words and two seeds. Quick and dirty.
fn weak_hash_len_32_with_seeds_words(w: u64, x: u64, y: u64, z: u64, a: u64, b: u64) -> (u64, u64) {
    let a = a.wrapping_add(w);
    let b = b.wrapping_add(a).wrapping_add(z).rotate_right(21);
    let c = a;
    let a = a.wrapping_add(x).wrapping_add(y);
    let b = b.wrapping_add(a.rotate_right(44));
    (a.wrapping_add(z), b.wrapping_add(c))
}

/// A 16-byte hash of the 32 bytes of `s` at `i`, and two seeds.
fn weak_hash_len_32_with_seeds(s: &[u8], i: usize, a: u64, b: u64) -> (u64, u64) {
    weak_hash_len_32_with_seeds_words(
        fetch64(s, i),
        fetch64(s, i + 8),
        fetch64(s, i + 16),
        fetch64(s, i + 24),
        a,
        b,
    )
}

fn hash_len_33_to_64(s: &[u8]) -> u64 {
    let slen = s.len();
    let mul = K2.wrapping_add((slen as u64).wrapping_mul(2));
    let a = fetch64(s, 0).wrapping_mul(K2);
    let b = fetch64(s, 8);
    let c = fetch64(s, slen - 8).wrapping_mul(mul);
    let d = fetch64(s, slen - 16).wrapping_mul(K2);
    let y = a
        .wrapping_add(b)
        .rotate_right(43)
        .wrapping_add(c.rotate_right(30))
        .wrapping_add(d);
    let z = hash_len_16_mul(
        y,
        a.wrapping_add(b.wrapping_add(K2).rotate_right(18))
            .wrapping_add(c),
        mul,
    );
    let e = fetch64(s, 16).wrapping_mul(mul);
    let f = fetch64(s, 24);
    let g = y.wrapping_add(fetch64(s, slen - 32)).wrapping_mul(mul);
    let h = z.wrapping_add(fetch64(s, slen - 24)).wrapping_mul(mul);
    hash_len_16_mul(
        e.wrapping_add(f)
            .rotate_right(43)
            .wrapping_add(g.rotate_right(30))
            .wrapping_add(h),
        e.wrapping_add(f.wrapping_add(a).rotate_right(18))
            .wrapping_add(g),
        mul,
    )
}

/// naHash64 over more than 64 bytes: a loop over 64-byte blocks with 56 bytes of state (`v`, `w`, `x`, `y`, `z`), then
/// the last 64 bytes of `s`, which overlap the last block the loop read.
fn hash_len_65_plus(s: &[u8]) -> u64 {
    const SEED: u64 = 81;
    let slen = s.len();
    let (mut v_lo, mut v_hi) = (0u64, 0u64);
    let (mut w_lo, mut w_hi) = (0u64, 0u64);
    let mut x = SEED.wrapping_mul(K2).wrapping_add(fetch64(s, 0));
    let mut y = SEED.wrapping_mul(K1).wrapping_add(113);
    let mut z = shift_mix(y.wrapping_mul(K2).wrapping_add(113)).wrapping_mul(K2);
    // Go loops while more than 64 bytes remain, which leaves 1 to 64 bytes after the loop.
    for block in s.as_chunks::<64>().0.iter().take((slen - 1) / 64) {
        x = x
            .wrapping_add(y)
            .wrapping_add(v_lo)
            .wrapping_add(fetch64(block, 8))
            .rotate_right(37)
            .wrapping_mul(K1);
        y = y
            .wrapping_add(v_hi)
            .wrapping_add(fetch64(block, 48))
            .rotate_right(42)
            .wrapping_mul(K1);
        x ^= w_hi;
        y = y.wrapping_add(v_lo).wrapping_add(fetch64(block, 40));
        z = z.wrapping_add(w_lo).rotate_right(33).wrapping_mul(K1);
        (v_lo, v_hi) =
            weak_hash_len_32_with_seeds(block, 0, v_hi.wrapping_mul(K1), x.wrapping_add(w_lo));
        (w_lo, w_hi) = weak_hash_len_32_with_seeds(
            block,
            32,
            z.wrapping_add(w_hi),
            y.wrapping_add(fetch64(block, 16)),
        );
        std::mem::swap(&mut x, &mut z);
    }
    let mul = K1.wrapping_add((z & 0xff) << 1);
    let last = slen - 64;
    w_lo = w_lo.wrapping_add((slen as u64 - 1) & 63);
    v_lo = v_lo.wrapping_add(w_lo);
    w_lo = w_lo.wrapping_add(v_lo);
    x = x
        .wrapping_add(y)
        .wrapping_add(v_lo)
        .wrapping_add(fetch64(s, last + 8))
        .rotate_right(37)
        .wrapping_mul(mul);
    y = y
        .wrapping_add(v_hi)
        .wrapping_add(fetch64(s, last + 48))
        .rotate_right(42)
        .wrapping_mul(mul);
    x ^= w_hi.wrapping_mul(9);
    y = y
        .wrapping_add(v_lo.wrapping_mul(9))
        .wrapping_add(fetch64(s, last + 40));
    z = z.wrapping_add(w_lo).rotate_right(33).wrapping_mul(mul);
    (v_lo, v_hi) =
        weak_hash_len_32_with_seeds(s, last, v_hi.wrapping_mul(mul), x.wrapping_add(w_lo));
    (w_lo, w_hi) = weak_hash_len_32_with_seeds(
        s,
        last + 32,
        z.wrapping_add(w_hi),
        y.wrapping_add(fetch64(s, last + 16)),
    );
    std::mem::swap(&mut x, &mut z);
    hash_len_16_mul(
        hash_len_16_mul(v_lo, w_lo, mul)
            .wrapping_add(shift_mix(y).wrapping_mul(K0))
            .wrapping_add(z),
        hash_len_16_mul(v_hi, w_hi, mul).wrapping_add(x),
        mul,
    )
}
