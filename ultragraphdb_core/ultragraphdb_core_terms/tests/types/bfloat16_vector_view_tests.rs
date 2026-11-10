/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::{EncodedTerm, TermView};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// `PROTOCOL.md` §10.1, tag `0x41`: the dimension, a little-endian u16, then that many little-endian bfloat16s, each the
/// high 16 bits of an IEEE 754 binary32. Here dimension 3: 1 (0x3F80, bytes `80 3F`), -2 (0xC000, `00 C0`) and 0.15625
/// (0x3E20, `20 3E`), each exact in 8 bits of significand.
#[test]
fn test_a_view_reads_the_elements_in_place() -> TestResult {
    let bytes = [0x41, 3, 0, 0x80, 0x3F, 0x00, 0xC0, 0x20, 0x3E];
    let term = EncodedTerm::from_bytes(&bytes)?;
    let view = match term.result_view()? {
        TermView::Bfloat16Vector(view) => Some(view),
        _ => None,
    }
    .context("a bfloat16vector")?;
    assert_eq!((view.len(), view.is_empty()), (3, false));
    assert_eq!(view.iter().collect::<Vec<_>>(), vec![1.0, -2.0, 0.156_25]);
    assert_eq!(view.as_bytes(), &bytes[3..]);
    Ok(())
}
