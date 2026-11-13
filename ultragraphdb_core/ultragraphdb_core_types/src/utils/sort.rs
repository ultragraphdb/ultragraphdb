/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/sort.go. Changed for ultragraphdb: translated to Rust; the language collator is
 * a parameter (the Collator trait); rows are ordered by the standard library's sort under a total order, stably;
 * `SortWithFacet`, a query-layer function of protobuf facets, is not ported.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::cmp::Ordering;

use ultragraphdb_traits_collator::Collator;

use crate::errors::types_error::TypesError;
use crate::types::text_collator::TextCollator;
use crate::types::type_id::TypeId;
use crate::types::val::Val;
use crate::types::value::Value;

/// Dgraph's `cl`: the collator a sort compares strings with, and the language it collates.
struct Collation<'c, C: Collator> {
    collator: &'c C,
    language: C::Language,
}

/// How two rows order: key by key, each in its own direction, a row's missing or nil key after every value
/// whatever the direction, and the next key deciding between keys that are equal or both nil. A total order, so
/// the standard library's sorts take it for any rows.
fn cmp_rows<C: Collator>(
    first: &[Val],
    second: &[Val],
    desc: &[bool],
    cl: Option<&Collation<'_, C>>,
) -> Ordering {
    (0..first.len().max(second.len()))
        .map(|at| match (row_key(first, at), row_key(second, at)) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) => {
                let order = cmp_vals(a, b, cl);
                if desc.get(at).copied().unwrap_or(false) {
                    order.reverse()
                } else {
                    order
                }
            }
        })
        .find(|order| order.is_ne())
        .unwrap_or(Ordering::Equal)
}

/// A row's key at `at`, when the row holds a value there.
fn row_key(row: &[Val], at: usize) -> Option<&Val> {
    row.get(at).filter(|v| v.value().is_some())
}

/// A value's text; the empty text for a value that holds none.
fn text_of(v: &Val) -> &str {
    match v.value() {
        Some(Value::Str(s)) => s,
        _ => "",
    }
}

/// How two values order, totally: values of one type by that type, an int and a float by their exact numbers,
/// values of other types by type identifier. A NaN float goes past the infinity of its sign.
fn cmp_vals<C: Collator>(a: &Val, b: &Val, cl: Option<&Collation<'_, C>>) -> Ordering {
    match (a.value(), b.value()) {
        (Some(Value::Int(x)), Some(Value::Float(y))) => cmp_int_float(*x, *y),
        (Some(Value::Float(x)), Some(Value::Int(y))) => cmp_int_float(*y, *x).reverse(),
        _ if a.tid() != b.tid() => a.tid().cmp(&b.tid()),
        (Some(Value::DateTime(x)), Some(Value::DateTime(y))) => {
            if x.before(y) {
                Ordering::Less
            } else if y.before(x) {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        }
        (Some(Value::Int(x)), Some(Value::Int(y))) => x.cmp(y),
        (Some(Value::Float(x)), Some(Value::Float(y))) => {
            x.partial_cmp(y).unwrap_or_else(|| x.total_cmp(y))
        }
        (Some(Value::Uid(x)), Some(Value::Uid(y))) => x.cmp(y),
        (Some(Value::Decimal(x)), Some(Value::Decimal(y))) => x.cmp(y),
        _ if matches!(a.tid(), TypeId::STRING | TypeId::DEFAULT) => {
            let (sa, sb) = (text_of(a), text_of(b));
            match cl {
                Some(cl) => cl
                    .collator
                    .compare_bytes(&cl.language, sa.as_bytes(), sb.as_bytes()),
                None => sa.cmp(sb),
            }
        }
        _ => Ordering::Equal,
    }
}

/// How an int orders against a float, by their exact numbers: the float's rounding of the int decides unless the
/// two are equal there, where the float is whole and the ints decide.
fn cmp_int_float(x: i64, y: f64) -> Ordering {
    // Two to the 63rd, the least float past every int.
    const PAST_EVERY_INT: f64 = 9_223_372_036_854_775_808.0;
    if y.is_nan() {
        return if y.is_sign_negative() {
            Ordering::Greater
        } else {
            Ordering::Less
        };
    }
    match (x as f64).partial_cmp(&y) {
        Some(Ordering::Less) => Ordering::Less,
        Some(Ordering::Greater) => Ordering::Greater,
        Some(Ordering::Equal) | None if y >= PAST_EVERY_INT => Ordering::Less,
        // `y` is whole and within an int's range here, so the cast is its value.
        Some(Ordering::Equal) | None => x.cmp(&(y as i64)),
    }
}

/// Moves each row and its uid to the place `order` gives it: place `i` takes row `order[i]`.
fn arrange(order: &[usize], values: &mut [Vec<Val>], uids: &mut [u64]) {
    let rows: Vec<Vec<Val>> = order
        .iter()
        .map(|&from| values.get_mut(from).map(std::mem::take).unwrap_or_default())
        .collect();
    let ids: Vec<u64> = order
        .iter()
        .filter_map(|&from| uids.get(from).copied())
        .collect();
    values
        .iter_mut()
        .zip(rows)
        .for_each(|(slot, row)| *slot = row);
    uids.iter_mut().zip(ids).for_each(|(slot, id)| *slot = id);
}

/// Whether a row's first key is missing or nil.
fn is_nil(row: &[Val]) -> bool {
    row.first().is_none_or(|v| v.value().is_none())
}

/// Dgraph's `IsSortable`: datetime, int, float, string, default and decimal.
pub fn is_sortable(tid: TypeId) -> bool {
    matches!(
        tid,
        TypeId::DATETIME
            | TypeId::INT
            | TypeId::FLOAT
            | TypeId::STRING
            | TypeId::DEFAULT
            | TypeId::DECIMAL
    )
}

/// Refuses rows Dgraph's sort would index past: a non-empty row shorter or longer than the first, fewer
/// directions than the first row has values, and uids other than one per row (`exact_uids`) or fewer.
fn check_shape(
    values: &[Vec<Val>],
    uids: usize,
    desc: usize,
    exact_uids: bool,
) -> Result<(), TypesError> {
    let width = values.first().map_or(0, Vec::len);
    if let Some((i, row)) = values
        .iter()
        .enumerate()
        .find(|(_, row)| !row.is_empty() && row.len() != width)
    {
        return Err(TypesError::compare(format!(
            "Sort row {i} holds {} values where the first holds {width}",
            row.len()
        )));
    }
    if uids < values.len() || exact_uids && uids != values.len() {
        return Err(TypesError::compare(format!(
            "Sort has {} rows of values but {uids} uids",
            values.len()
        )));
    }
    if desc < width {
        return Err(TypesError::compare(format!(
            "Sort has {desc} directions for rows of {width} values"
        )));
    }
    Ok(())
}

fn check_sortable(values: &[Vec<Val>]) -> Result<(), TypesError> {
    values.iter().take(1).flatten().try_for_each(|v| {
        if is_sortable(v.tid()) {
            Ok(())
        } else {
            Err(TypesError::compare(format!(
                "Value of type: {} isn't sortable",
                v.tid().name()
            )))
        }
    })
}

/// The collation of `lang`; `None` when `lang` is empty or the collator refuses it, and the sort compares bytes.
fn collation<'c, C: Collator>(lang: &str, collator: &'c C) -> Option<Collation<'c, C>> {
    if lang.is_empty() {
        return None;
    }
    collator
        .language(lang)
        .map(|language| Collation { collator, language })
}

/// Dgraph's `SortTopN`: moves the `n` smallest rows (nil values last) to the front, in order, with their uids.
///
/// The rows after the first `n` hold the rest in no stated order, and `n` of zero sorts no row. `n` is a limit,
/// read as SQL's `LIMIT` and Dgraph's own `first:` read one: a count at or past the row count sorts every row.
/// Rows that compare equal keep the order they came in when every row is sorted.
pub fn sort_top_n<C: Collator>(
    values: &mut [Vec<Val>],
    uids: &mut [u64],
    desc: &[bool],
    lang: &str,
    n: usize,
    collator: &C,
) -> Result<(), TypesError> {
    if values.first().is_none_or(Vec::is_empty) {
        return Ok(());
    }
    check_sortable(values)?;
    check_shape(values, uids.len(), desc.len(), true)?;
    let cl = collation(lang, collator);
    let rows: &[Vec<Val>] = values;
    let row = |at: usize| rows.get(at).map_or(&[][..], Vec::as_slice);
    let cmp = |a: &usize, b: &usize| cmp_rows(row(*a), row(*b), desc, cl.as_ref());
    // The rows with a first key, in the order they came in, then the rows without.
    let (mut order, without): (Vec<usize>, Vec<usize>) =
        (0..rows.len()).partition(|&at| !is_nil(row(at)));
    let with = order.len();
    order.extend(without);
    if n < with {
        // Fewer are wanted than have a key: the `n` smallest of those come to the front first.
        if let Some(keyed) = order.get_mut(..with) {
            let _parts = keyed.select_nth_unstable_by(n, cmp);
        }
    }
    if let Some(front) = order.get_mut(..n.min(rows.len())) {
        front.sort_by(cmp);
    }
    arrange(&order, values, uids);
    Ok(())
}

/// Dgraph's `Sort`: sorts rows of values in place with their uids; a nil value sorts last. Rows that compare equal
/// keep the order they came in.
pub fn sort<C: Collator>(
    values: &mut [Vec<Val>],
    uids: &mut [u64],
    desc: &[bool],
    lang: &str,
    collator: &C,
) -> Result<(), TypesError> {
    if values.first().is_none_or(Vec::is_empty) {
        return Ok(());
    }
    check_sortable(values)?;
    check_shape(values, uids.len(), desc.len(), false)?;
    let cl = collation(lang, collator);
    let rows: &[Vec<Val>] = values;
    let row = |at: usize| rows.get(at).map_or(&[][..], Vec::as_slice);
    let mut order: Vec<usize> = (0..rows.len()).collect();
    order.sort_by(|a, b| cmp_rows(row(*a), row(*b), desc, cl.as_ref()));
    arrange(&order, values, uids);
    Ok(())
}

/// Dgraph's `Less`: whether `a` is strictly less than `b`, for values of one sortable type (uid included).
pub fn less(a: &Val, b: &Val) -> Result<bool, TypesError> {
    if a.tid() != b.tid() {
        return Err(TypesError::compare(
            "Arguments of different type can not be compared.",
        ));
    }
    match a.tid() {
        TypeId::DATETIME
        | TypeId::UID
        | TypeId::INT
        | TypeId::FLOAT
        | TypeId::STRING
        | TypeId::DEFAULT
        | TypeId::DECIMAL => Ok(less_vals::<TextCollator>(a, b, None)),
        t => Err(TypesError::compare(format!(
            "Compare not supported for type: {}",
            t.enum_value()
        ))),
    }
}

fn less_vals<C: Collator>(a: &Val, b: &Val, cl: Option<&Collation<'_, C>>) -> bool {
    if a.tid() != b.tid() {
        return mismatched_less(a, b);
    }
    match (a.value(), b.value()) {
        (Some(Value::DateTime(x)), Some(Value::DateTime(y))) => x.before(y),
        (Some(Value::Int(x)), Some(Value::Int(y))) => x < y,
        (Some(Value::Float(x)), Some(Value::Float(y))) => x < y,
        (Some(Value::Uid(x)), Some(Value::Uid(y))) => x < y,
        (Some(Value::Decimal(x)), Some(Value::Decimal(y))) => x < y,
        _ if matches!(a.tid(), TypeId::STRING | TypeId::DEFAULT) => {
            let sa = match a.safe() {
                Some(Value::Str(s)) => s,
                _ => String::new(),
            };
            let sb = match b.safe() {
                Some(Value::Str(s)) => s,
                _ => String::new(),
            };
            // Use language comparator.
            match cl {
                Some(cl) => {
                    cl.collator
                        .compare_bytes(&cl.language, sa.as_bytes(), sb.as_bytes())
                        == Ordering::Less
                }
                None => sa < sb,
            }
        }
        _ => false,
    }
}

/// Dgraph's `mismatchedLess`: ints and floats compare as floats; other types order by type identifier.
fn mismatched_less(a: &Val, b: &Val) -> bool {
    match (a.value(), b.value()) {
        (Some(Value::Float(x)), Some(Value::Int(y)))
            if a.tid() == TypeId::FLOAT && b.tid() == TypeId::INT =>
        {
            *x < *y as f64
        }
        (Some(Value::Int(x)), Some(Value::Float(y)))
            if a.tid() == TypeId::INT && b.tid() == TypeId::FLOAT =>
        {
            (*x as f64) < *y
        }
        _ => a.tid() < b.tid(),
    }
}

/// Dgraph's `Equal`: whether two values of one comparable type are equal.
pub fn equal(a: &Val, b: &Val) -> Result<bool, TypesError> {
    if a.tid() != b.tid() {
        return Err(TypesError::compare(
            "Arguments of different type can not be compared.",
        ));
    }
    match a.tid() {
        TypeId::DATETIME
        | TypeId::INT
        | TypeId::FLOAT
        | TypeId::STRING
        | TypeId::DEFAULT
        | TypeId::BOOL
        | TypeId::DECIMAL => Ok(equal_vals(a, b)),
        t => Err(TypesError::compare(format!(
            "Equal not supported for type: {}",
            t.enum_value()
        ))),
    }
}

/// Dgraph's `equal`: false for different types, nil values, or a value of another Go type than its identifier's.
pub(crate) fn equal_vals(a: &Val, b: &Val) -> bool {
    if a.tid() != b.tid() {
        return false;
    }
    match (a.tid(), a.value(), b.value()) {
        (TypeId::DATETIME, Some(Value::DateTime(x)), Some(Value::DateTime(y))) => x.equal(y),
        (TypeId::INT, Some(Value::Int(x)), Some(Value::Int(y))) => x == y,
        (TypeId::FLOAT, Some(Value::Float(x)), Some(Value::Float(y))) => x == y,
        (TypeId::STRING | TypeId::DEFAULT, Some(Value::Str(x)), Some(Value::Str(y))) => x == y,
        (TypeId::BOOL, Some(Value::Bool(x)), Some(Value::Bool(y))) => x == y,
        (TypeId::DECIMAL, Some(Value::Decimal(x)), Some(Value::Decimal(y))) => x == y,
        _ => false,
    }
}
