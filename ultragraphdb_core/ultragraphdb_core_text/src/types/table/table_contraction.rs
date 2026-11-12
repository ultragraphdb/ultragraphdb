/*
 * Copyright 2012 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/colltab/table.go (matchContraction). Changed for ultragraphdb:
 * translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use super::Table;
use crate::tables::collate_tables::MAIN_CONTRACT_ELEM;
use crate::types::ct_scanner::CtScanner;
use crate::types::elem::{CeType, Elem};
use crate::utils::norm::{MAX_SEGMENT_SIZE, first_boundary, first_rune, nfd_properties};

const RUNE_SELF: u8 = 0x80;

impl Table {
    /// Appends the elements of the longest contraction starting with the rune of `ce` and matching `suffix`, the
    /// input after that rune, and of the runes it skipped; returns the bytes of `suffix` consumed.
    pub(crate) fn match_contraction(&self, w: &mut Vec<Elem>, ce: Elem, suffix: &[u8]) -> usize {
        let (index, n, offset) = ce.split_contract_index();

        let mut scan = CtScanner::new(index, n, suffix);
        let mut buf: Vec<u8> = Vec::with_capacity(MAX_SEGMENT_SIZE);
        let mut bufp = 0;
        let mut p = scan.scan(0);

        // The byte of `suffix` at `at` starts a multi-byte rune.
        let multi_byte_at = |at: usize| suffix.get(at).is_some_and(|&b| b >= RUNE_SELF);
        if !scan.done && multi_byte_at(p) {
            // By now we should have filtered most cases. Invalid UTF-8 has the properties of a starter in norm's
            // trie, so it ends the scan, and its width, Go's and this one alike, is never used.
            let mut p0 = p;
            let (r, size) = first_rune(suffix.get(p..).unwrap_or_default());
            let mut rune = nfd_properties(r);
            p += size;
            if rune.lead_ccc != 0 {
                let mut prev_cc = rune.trail_ccc;
                // A gap may only occur in the last normalization segment. This also ensures that the scanned input
                // is shorter than a segment.
                if let Some(end) = first_boundary(suffix.get(p..).unwrap_or_default()) {
                    scan.s_end = p + end;
                }
                while !scan.done && multi_byte_at(p) {
                    let (next_r, next_size) = first_rune(suffix.get(p..).unwrap_or_default());
                    rune = nfd_properties(next_r);
                    let ccc = rune.lead_ccc;
                    if ccc == 0 || prev_cc >= ccc {
                        break;
                    }
                    prev_cc = rune.trail_ccc;
                    let pp = scan.scan(p);
                    if pp != p {
                        // Copy the interstitial runes for later processing; Go's buffer holds one segment.
                        let room = MAX_SEGMENT_SIZE - buf.len();
                        let part = suffix.get(p0..p).unwrap_or_default();
                        buf.extend(part.iter().take(room));
                        if scan.pindex == pp {
                            bufp = buf.len();
                        }
                        p = pp;
                        p0 = pp;
                    } else {
                        p += next_size;
                    }
                }
            }
        }
        // Append weights for the matched contraction, which may be an expansion.
        let (i, consumed) = scan.result();
        let contraction_ce = Elem(
            MAIN_CONTRACT_ELEM
                .get(i + offset)
                .copied()
                .unwrap_or_default(),
        );
        if contraction_ce.ctype() == CeType::Normal {
            w.push(contraction_ce);
        } else {
            self.append_expansion(w, contraction_ce);
        }
        // Append weights for the runes in the segment not part of the contraction.
        let mut b = buf.get(..bufp).unwrap_or_default();
        while !b.is_empty() {
            let rune_len = self.append_next(w, b);
            if rune_len == 0 {
                // An incomplete rune at the end of a full buffer, where Go loops forever.
                break;
            }
            b = b.get(rune_len..).unwrap_or_default();
        }
        consumed
    }
}
