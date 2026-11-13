/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The canonical hex dump, as `hexdump -C` writes it: sixteen bytes a line, each line its offset in eight hex digits, the bytes in hex with an extra
/// space after the eighth, and the printable ASCII characters between bars.
pub fn hex_dump(data: &[u8]) -> String {
    data.chunks(16)
        .enumerate()
        .map(|(line, chunk)| {
            let bytes: String = (0..16)
                .map(|i| {
                    let cell = chunk
                        .get(i)
                        .map_or("  ".to_string(), |b| format!("{b:02x}"));
                    if i == 7 {
                        format!("{cell}  ")
                    } else {
                        format!("{cell} ")
                    }
                })
                .collect();
            let chars: String = chunk
                .iter()
                .map(|&b| {
                    if (32..=126).contains(&b) {
                        b as char
                    } else {
                        '.'
                    }
                })
                .collect();
            format!("{:08x}  {bytes} |{chars}|\n", line * 16)
        })
        .collect()
}
