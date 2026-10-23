/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::Entry;

use crate::types::drop_record::DropRecord;

/// Marks an entry that is a deletion; a value carries its meta byte and discard flag instead.
const ENTRY_DELETION: u8 = 0x00;
const ENTRY_PUT: u8 = 0x01;
const ENTRY_PUT_DISCARD: u8 = 0x02;

impl DropRecord {
    /// The record's bytes: `[0 = prefixes | 1 = everything][ts]`, the prefixes as a count and length-prefixed bytes,
    /// then the entries as a count and, for each, its key, version, kind, meta byte and value. Integers are
    /// big-endian, counts and lengths four bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![u8::from(self.prefixes.is_none())];
        out.extend_from_slice(&self.ts.to_be_bytes());
        let prefixes = self.prefixes.as_deref().unwrap_or_default();
        push_len(&mut out, prefixes.len());
        prefixes.iter().for_each(|p| push_bytes(&mut out, p));
        push_len(&mut out, self.entries.len());
        for e in &self.entries {
            push_bytes(&mut out, e.key());
            out.extend_from_slice(&e.version().to_be_bytes());
            out.push(match (e.is_deletion(), e.discard()) {
                (true, _) => ENTRY_DELETION,
                (false, false) => ENTRY_PUT,
                (false, true) => ENTRY_PUT_DISCARD,
            });
            out.push(e.meta());
            push_bytes(&mut out, e.value().unwrap_or_default());
        }
        out
    }

    /// The record `bytes` encode, or why they do not encode one.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut r = Reader { bytes };
        let everything = match r.byte()? {
            0 => false,
            1 => true,
            other => return Err(format!("unknown drop kind {other}")),
        };
        let ts = r.u64()?;
        let prefixes = (0..r.len()?)
            .map(|_| r.bytes().map(<[u8]>::to_vec))
            .collect::<Result<Vec<_>, _>>()?;
        let entries = (0..r.len()?)
            .map(|_| {
                let key = r.bytes()?.to_vec();
                let version = r.u64()?;
                let kind = r.byte()?;
                let meta = r.byte()?;
                let value = r.bytes()?.to_vec();
                match kind {
                    ENTRY_DELETION => Ok(Entry::deletion(key, version)),
                    ENTRY_PUT => Ok(Entry::put(key, version, meta, value)),
                    ENTRY_PUT_DISCARD => Ok(Entry::put(key, version, meta, value).with_discard()),
                    other => Err(format!("unknown entry kind {other}")),
                }
            })
            .collect::<Result<Vec<_>, String>>()?;
        if !r.bytes.is_empty() {
            return Err(format!("{} bytes after the drop record", r.bytes.len()));
        }
        Ok(Self {
            ts,
            prefixes: (!everything).then_some(prefixes),
            entries,
        })
    }
}

fn push_len(out: &mut Vec<u8>, len: usize) {
    // A drop's prefixes and entries, and each one's bytes, are far below 4 GiB: keys are at most 65,000 bytes and a
    // drop writes a namespace's initial schema.
    out.extend_from_slice(&u32::try_from(len).unwrap_or(u32::MAX).to_be_bytes());
}

fn push_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    push_len(out, bytes.len());
    out.extend_from_slice(bytes);
}

/// Reads a record's fields in order, refusing a record that ends early.
struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.bytes.len() < n {
            return Err(format!(
                "drop record ends after {} of {n} bytes",
                self.bytes.len()
            ));
        }
        let (head, rest) = self.bytes.split_at(n);
        self.bytes = rest;
        Ok(head)
    }

    fn byte(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u64(&mut self) -> Result<u64, String> {
        let mut buf = [0u8; 8];
        buf.copy_from_slice(self.take(8)?);
        Ok(u64::from_be_bytes(buf))
    }

    fn len(&mut self) -> Result<usize, String> {
        let mut buf = [0u8; 4];
        buf.copy_from_slice(self.take(4)?);
        Ok(u32::from_be_bytes(buf) as usize)
    }

    fn bytes(&mut self) -> Result<&'a [u8], String> {
        let n = self.len()?;
        self.take(n)
    }
}
