/*
 * Copyright 2018 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27.0, src/math/trig_reduce.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Payne-Hanek argument reduction for large arguments, as Go's `math.trigReduce`.

use super::bits::{BIAS, MASK, SHIFT};

/// The threshold above which Go's sin and cos use Payne-Hanek reduction (`1 << 29`).
pub(crate) const REDUCE_THRESHOLD: f64 = (1u64 << 29) as f64;

// Go's constant `Pi / 4`.
const PI4: f64 = f64::from_bits(0x3fe921fb54442d18);

/// Implements Payne-Hanek range reduction by Pi/4 for x > 0. It returns the integer part mod 8
/// (j) and the fractional part (z) of x / (Pi/4).
/// `None` only for an exponent outside the table, which a finite x of at least Pi/4 never has.
pub(crate) fn trig_reduce(x: f64) -> Option<(u64, f64)> {
    if x < PI4 {
        return Some((0, x));
    }
    // Extract out the integer and exponent such that, x = ix * 2 ** exp.
    let mut ix = x.to_bits();
    // The masked exponent field is 11 bits wide, so reading it as signed keeps its value.
    let exp = ((ix >> SHIFT) & MASK).cast_signed() - BIAS - i64::from(SHIFT);
    ix &= !(MASK << SHIFT);
    ix |= 1 << SHIFT;
    // Use the exponent to extract the 3 appropriate uint64 digits from mPi4,
    // B ~ (z0, z1, z2), such that the product leading digit has the exponent -61.
    // Note, exp >= -53 since x >= PI4 and exp < 971 for maximum float64.
    // Go's uint(exp+61): positive, as exp >= -53, and at most 1032, so its four digits lie within the table.
    let off = usize::try_from(exp + 61).ok()?;
    let bitshift = u32::try_from(off % 64).ok()?;
    let &[d0, d1, d2, d3] = M_PI4.get(off / 64..)?.first_chunk::<4>()?;
    let z0 = (d0 << bitshift) | shr(d1, 64 - bitshift);
    let z1 = (d1 << bitshift) | shr(d2, 64 - bitshift);
    let z2 = (d2 << bitshift) | shr(d3, 64 - bitshift);
    // Multiply mantissa by the digits and extract the upper two digits (hi, lo).
    let (z2hi, _) = mul64(z2, ix);
    let (z1hi, z1lo) = mul64(z1, ix);
    let z0lo = z0.wrapping_mul(ix);
    let (lo, c) = z1lo.overflowing_add(z2hi);
    let hi = z0lo.wrapping_add(z1hi).wrapping_add(u64::from(c));
    // The top 3 bits of hi give j.
    let mut j = hi >> 61;
    // Extract the fraction and find its magnitude.
    let hi = (hi << 3) | (lo >> 61);
    let lz = hi.leading_zeros();
    let e = (BIAS as u64).wrapping_sub(u64::from(lz) + 1); // lossless: BIAS is 1023
    // Clear implicit mantissa bit and shift into place.
    // Go's uint subtraction wraps when hi is 0, and its shift by the result gives 0.
    let mut hi = shl(hi, lz + 1) | shr(lo, 64u32.wrapping_sub(lz + 1));
    hi >>= 64 - SHIFT;
    // Include the exponent and convert to a float.
    hi |= e << SHIFT;
    let mut z = f64::from_bits(hi);
    // Map zeros to origin.
    if j & 1 == 1 {
        j += 1;
        j &= 7;
        z -= 1.0;
    }
    // Multiply the fractional part by pi/4.
    Some((j, z * PI4))
}

/// Go's `<<` on uint64, which yields 0 for shifts of 64 or more.
fn shl(x: u64, s: u32) -> u64 {
    if s >= 64 { 0 } else { x << s }
}

/// Go's `>>` on uint64, which yields 0 for shifts of 64 or more.
fn shr(x: u64, s: u32) -> u64 {
    if s >= 64 { 0 } else { x >> s }
}

/// Go's `bits.Mul64`: the 128-bit product as (hi, lo).
fn mul64(x: u64, y: u64) -> (u64, u64) {
    let p = u128::from(x) * u128::from(y);
    // The low half of the product is the wrapping product.
    ((p >> 64) as u64, x.wrapping_mul(y))
}

/// The binary digits of 4/pi as a uint64 array, that is, 4/pi = Sum mPi4[i]*2^(-64*i).
/// 19 64-bit digits and the leading one bit give 1217 bits of precision.
const M_PI4: [u64; 20] = [
    0x0000000000000001,
    0x45f306dc9c882a53,
    0xf84eafa3ea69bb81,
    0xb6c52b3278872083,
    0xfca2c757bd778ac3,
    0x6e48dc74849ba5c0,
    0x0c925dd413a32439,
    0xfc3bd63962534e7d,
    0xd1046bea5d768909,
    0xd338e04d68befc82,
    0x7323ac7306a673e9,
    0x3908bf177bf25076,
    0x3ff12fffbc0b301f,
    0xde5e2316b414da3e,
    0xda6cfd9e4f96136e,
    0x9e8c7ecd3cbfd45a,
    0xea4f758fd7cbe2f6,
    0x7a0e73ef14a525d4,
    0xd7f6bf623f1aba10,
    0xac06608df8f6d757,
];
