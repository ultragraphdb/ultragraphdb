/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, x/keys.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::errors::key_error::{KeyError, KeyErrorEnum};
use crate::keys::namespace_attr::namespace_attr;
use crate::keys::{
    BYTE_COUNT, BYTE_COUNT_REV, BYTE_DATA, BYTE_INDEX, BYTE_REVERSE, BYTE_SCHEMA, BYTE_SPLIT,
    BYTE_TYPE, BYTE_UNUSED,
};
use crate::types::parsed_key::ParsedKey;

/// The u64 in the first eight bytes; every caller checks that `bytes` holds at least eight.
fn be_u64(bytes: &[u8]) -> u64 {
    u64::from_be_bytes(
        bytes[..8]
            .try_into()
            .expect("a slice of exactly eight bytes"),
    )
}

/// Parses a key into its parts. The parsed key does not borrow the key.
///
/// A key with the prefix [`BYTE_UNUSED`] parses to an otherwise empty key. Bytes after the suffix of an unsplit
/// data, reverse or count key are ignored, and an index term takes every byte after its type byte.
///
/// # Errors
///
/// With Dgraph's error texts, which print the key past its prefix and namespace: a key shorter than 9 bytes, an
/// attribute longer than the key, an attribute that is not UTF-8 (which Dgraph's Go strings accept), a uid shorter
/// than 8 bytes or of 0, a count shorter than 4 bytes, a split key whose start uid is not exactly 8 bytes (at least
/// 8 for an index key), and an unknown data type.
///
/// A key that ends right after its attribute and is not a schema or type key is refused with
/// [`InvalidFormat`](KeyErrorEnum::InvalidFormat), as every other truncation is; Dgraph panics on it (ledger 6).
pub fn parse(key: &[u8]) -> Result<ParsedKey, KeyError> {
    if key.len() < 9 {
        return Err(KeyError::new(KeyErrorEnum::KeyTooShort));
    }
    let mut p = ParsedKey {
        byte_prefix: key[0],
        ..ParsedKey::default()
    };
    let namespace = be_u64(&key[1..9]);
    let key = &key[9..];
    if p.byte_prefix == BYTE_UNUSED {
        return Ok(p);
    }

    p.has_start_uid = p.byte_prefix == BYTE_SPLIT;

    if key.len() < 3 {
        return Err(KeyError::new(KeyErrorEnum::InvalidFormat {
            key: key.to_vec(),
        }));
    }
    let sz = usize::from(u16::from_be_bytes([key[0], key[1]]));
    let k = &key[2..];

    if k.len() < sz {
        return Err(KeyError::new(KeyErrorEnum::InvalidSize {
            size: sz,
            key: key.to_vec(),
        }));
    }
    let attr = std::str::from_utf8(&k[..sz])
        .map_err(|_| KeyError::new(KeyErrorEnum::AttributeNotUtf8 { key: key.to_vec() }))?;
    p.attr = namespace_attr(namespace, attr);
    let k = &k[sz..];

    if matches!(p.byte_prefix, BYTE_SCHEMA | BYTE_TYPE) {
        return Ok(p);
    }

    let Some((&byte_type, k)) = k.split_first() else {
        return Err(KeyError::new(KeyErrorEnum::InvalidFormat {
            key: key.to_vec(),
        }));
    };
    p.byte_type = byte_type;

    match p.byte_type {
        BYTE_DATA | BYTE_REVERSE => {
            if k.len() < 8 {
                return Err(KeyError::new(KeyErrorEnum::UidTooShort {
                    key: key.to_vec(),
                    parsed: p,
                }));
            }
            p.uid = be_u64(k);
            if p.uid == 0 {
                return Err(KeyError::new(KeyErrorEnum::ZeroUid { key: key.to_vec() }));
            }
            if p.has_start_uid {
                if k.len() != 16 {
                    return Err(KeyError::new(KeyErrorEnum::StartUidLength {
                        key: key.to_vec(),
                        parsed: p,
                    }));
                }
                p.start_uid = be_u64(&k[8..]);
            }
        }
        BYTE_INDEX => {
            if !p.has_start_uid {
                p.term = k.to_vec();
            } else {
                if k.len() < 8 {
                    return Err(KeyError::new(KeyErrorEnum::StartUidTooShort {
                        key: key.to_vec(),
                        parsed: p,
                    }));
                }
                let (term, start_uid) = k.split_at(k.len() - 8);
                p.term = term.to_vec();
                p.start_uid = be_u64(start_uid);
            }
        }
        BYTE_COUNT | BYTE_COUNT_REV => {
            if k.len() < 4 {
                return Err(KeyError::new(KeyErrorEnum::CountTooShort {
                    key: key.to_vec(),
                    parsed: p,
                }));
            }
            p.count = u32::from_be_bytes(
                k[..4]
                    .try_into()
                    .expect("a slice of exactly four bytes, the length checked above"),
            );
            if p.has_start_uid {
                if k.len() != 12 {
                    return Err(KeyError::new(KeyErrorEnum::StartUidLength {
                        key: key.to_vec(),
                        parsed: p,
                    }));
                }
                p.start_uid = be_u64(&k[4..]);
            }
        }
        // Some other data type.
        _ => return Err(KeyError::new(KeyErrorEnum::InvalidDataType)),
    }
    Ok(p)
}
