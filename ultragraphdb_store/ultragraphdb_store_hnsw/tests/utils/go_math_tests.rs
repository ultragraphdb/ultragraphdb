/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_hnsw::insert_layer;

/// The level thresholds are Go's `math.Pow(1.0/5, n)`: a fifth of the nodes reach each level above the one below.
#[test]
fn test_the_insert_layer_follows_gos_thresholds() {
    assert_eq!(insert_layer(1, 0.999), 0);
    assert_eq!(insert_layer(3, 0.0), 0);
    // Go's math.Pow(0.2, 2) is 0.04000000000000001, as 0.2 * 0.2 rounds.
    assert_eq!(insert_layer(3, 0.04), 0);
    assert_eq!(insert_layer(3, 0.0401), 1);
    assert_eq!(insert_layer(3, 0.199), 1);
    assert_eq!(insert_layer(3, 0.2), 2);
    assert_eq!(insert_layer(3, 0.9999), 2);
    // 0.2^4 = 0.0016 in Go's arithmetic.
    assert_eq!(insert_layer(5, 0.00159), 0);
    assert_eq!(insert_layer(5, 0.0016001), 1);
    assert_eq!(
        insert_layer(4, f64::NAN),
        0,
        "no threshold passes, Go's zero level"
    );
}

/// Go 1.27's `math.Pow(1.0/5, n)` as IEEE bits (a probe of Go's math package): the thresholds of the normal range,
/// the subnormal range from n = 441, and the underflow to 0 from n = 463.
const GO_POW_FIFTH: [(usize, u64); 49] = [
    (1, 0x3fc999999999999a),
    (2, 0x3fa47ae147ae147c),
    (3, 0x3f80624dd2f1a9fd),
    (4, 0x3f5a36e2eb1c4330),
    (5, 0x3f34f8b588e368f4),
    (6, 0x3f10c6f7a0b5ed90),
    (7, 0x3eead7f29abcaf4e),
    (8, 0x3ec5798ee2308c3f),
    (9, 0x3ea12e0be826d699),
    (10, 0x3e7b7cdfd9d7bdc3),
    (16, 0x3d9cd2b297d889ca),
    (31, 0x3b7039d66589688f),
    (32, 0x3b49f623d5a8a74c),
    (33, 0x3b24c4e977ba1f70),
    (64, 0x36a50ffd44f4a766),
    (100, 0x316bff2ee48e0585),
    (200, 0x22e87e92154ef841),
    (300, 0x14656e1fc2f8f41c),
    (400, 0x05e2bfcfc0f924c2),
    (440, 0x00146a882d2c16d8),
    (441, 0x0004154e6f6f37c5),
    (442, 0x0000d10fafe30b27),
    (443, 0x000029cfeffa356e),
    (444, 0x0000085cc9987116),
    (445, 0x000001ac2851b038),
    (446, 0x00000055a1a9f00b),
    (447, 0x0000001120553002),
    (448, 0x000000036cddd667),
    (449, 0x00000000af5f9148),
    (450, 0x0000000023131d0e),
    (451, 0x000000000703d29c),
    (452, 0x0000000001672a1f),
    (453, 0x000000000047d539),
    (454, 0x00000000000e5dd8),
    (455, 0x000000000002df92),
    (456, 0x000000000000931d),
    (457, 0x0000000000001d6c),
    (458, 0x00000000000005e2),
    (459, 0x000000000000012d),
    (460, 0x000000000000003c),
    (461, 0x000000000000000c),
    (462, 0x0000000000000002),
    (463, 0),
    (464, 0),
    (1000, 0),
    (4095, 0),
    (4096, 0),
    (4097, 0),
    (5000, 0),
];

/// Each threshold equals Go's bit for bit: with `n + 1` levels, the top level's threshold is `0.2^n`, so a draw of
/// exactly Go's value misses it and lands one level down, and the draw just below it hits it. Where Go's power
/// underflows to 0 no draw hits a level whose threshold is 0: the first level reached is the first whose exponent is
/// 462 or less.
#[test]
fn test_the_thresholds_equal_gos_pow_bit_for_bit() {
    for (n, bits) in GO_POW_FIFTH {
        let t = f64::from_bits(bits);
        if bits > 0 {
            assert_eq!(insert_layer(n + 1, t), 1, "0.2^{n} = {t:e} misses");
            assert_eq!(
                insert_layer(n + 1, f64::from_bits(bits - 1)),
                0,
                "below 0.2^{n} = {t:e} hits"
            );
        } else {
            assert_eq!(insert_layer(n + 1, 0.0), n - 462, "0.2^{n} is 0 as in Go");
        }
    }
}
