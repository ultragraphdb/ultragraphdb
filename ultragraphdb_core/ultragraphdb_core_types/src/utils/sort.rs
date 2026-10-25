/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/sort.go. Changed for ultragraphdb: translated to Rust; the language collator is
 * a parameter (the Collator trait, TextCollator for golang.org/x/text/collate).
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::cmp::Ordering;

use ultragraphdb_proto_pb::pb;
use ultragraphdb_traits_collator::Collator;

use crate::errors::types_error::TypesError;
use crate::traits::sort_data::SortData;
use crate::types::text_collator::TextCollator;
use crate::types::type_id::TypeId;
use crate::types::val::Val;
use crate::types::value::Value;
use crate::utils::go_sort;
use crate::utils::select::quick_select;

/// Dgraph's `cl`: the collator a sort compares strings with, and the language it collates.
struct Collation<'c, C: Collator> {
    collator: &'c C,
    language: C::Language,
}

/// Dgraph's `byValue`: rows of values sorted together with their uids and facets.
struct ByValue<'a, C: Collator> {
    values: &'a mut [Vec<Val>],
    uids: &'a mut [u64],
    facets: Option<&'a mut [pb::Facets]>,
    desc: &'a [bool],
    cl: Option<Collation<'a, C>>,
    len: usize,
}

impl<C: Collator> SortData for ByValue<'_, C> {
    fn len(&self) -> usize {
        self.len
    }

    fn less(&self, i: usize, j: usize) -> bool {
        let (first, second) = (&self.values[i], &self.values[j]);
        if first.is_empty() || second.is_empty() {
            return false;
        }
        for vidx in 0..first.len() {
            let (a, b) = (&first[vidx], &second[vidx]);
            match (a.value(), b.value()) {
                (None, None) => return self.desc[vidx],
                (None, _) => return false,
                (_, None) => return true,
                _ => {}
            }
            if equal_vals(a, b) {
                continue;
            }
            let l = less_vals(a, b, self.cl.as_ref());
            return if self.desc[vidx] { !l } else { l };
        }
        false
    }

    fn swap(&mut self, i: usize, j: usize) {
        self.values.swap(i, j);
        self.uids.swap(i, j);
        if let Some(f) = self.facets.as_deref_mut() {
            f.swap(i, j);
        }
    }
}

impl<C: Collator> ByValue<'_, C> {
    fn is_nil(&self, i: usize) -> bool {
        self.values[i].first().is_none_or(|v| v.value().is_none())
    }
}

/// Dgraph's `IsSortable`: datetime, int, float, string, default and bigfloat.
pub fn is_sortable(tid: TypeId) -> bool {
    matches!(
        tid,
        TypeId::DATETIME
            | TypeId::INT
            | TypeId::FLOAT
            | TypeId::STRING
            | TypeId::DEFAULT
            | TypeId::BIGFLOAT
    )
}

/// Refuses rows Dgraph's sort would index past: a non-empty row shorter or longer than the first, fewer
/// directions than the first row has values, and uids or facets other than one per row (`exact_uids`) or fewer.
fn check_shape(
    values: &[Vec<Val>],
    uids: usize,
    facets: Option<usize>,
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
    if let Some(facets) = facets.filter(|&f| f < values.len()) {
        return Err(TypesError::compare(format!(
            "Sort has {} rows of values but {facets} facets",
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
/// The rows after the first `n` hold the rest in quickselect's order, as Go's do, and `n` of zero sorts no row.
/// `n` is a limit, read as SQL's `LIMIT` (PostgreSQL, SQLite) and Dgraph's own `first:` read one: a count at or
/// past the row count sorts every row, the order `n` equal to the row count gives, and no rows stay no rows for
/// any `n`. That is a deliberate choice. Go's `SortTopN` slices `values[:n]`, which panics past the slice's
/// capacity and, below it, sorts the empty rows past the end in with the real ones, so Go leaves the case
/// undefined; Dgraph's one caller, `processSort` in worker/sort.go, clamps `n` to the row count with
/// `x.PageRange` before the call. `n` is a `usize`, so a negative count, which Go's slice also panics on, is
/// not expressible.
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
    check_shape(values, uids.len(), None, desc.len(), true)?;
    let cl = collation(lang, collator);
    let len = values.len();
    let mut s = ByValue {
        values,
        uids,
        facets: None,
        desc,
        cl,
        len,
    };
    let count = s.uids.len();
    let mut nul = 0;
    for i in 0..count {
        if s.is_nil(i) {
            continue;
        }
        if i != nul {
            s.swap(i, nul);
        }
        nul += 1;
    }
    if nul > n {
        s.len = nul;
        quick_select(&mut s, 0, nul - 1, n);
    }
    s.len = n.min(len);
    go_sort::sort(&mut s);
    Ok(())
}

/// Dgraph's `SortWithFacet`: sorts rows of values in place with their uids and facets; a nil value sorts last.
pub fn sort_with_facet<C: Collator>(
    values: &mut [Vec<Val>],
    uids: &mut [u64],
    facets: Option<&mut [pb::Facets]>,
    desc: &[bool],
    lang: &str,
    collator: &C,
) -> Result<(), TypesError> {
    if values.first().is_none_or(Vec::is_empty) {
        return Ok(());
    }
    check_sortable(values)?;
    check_shape(
        values,
        uids.len(),
        facets.as_deref().map(<[pb::Facets]>::len),
        desc.len(),
        false,
    )?;
    let cl = collation(lang, collator);
    let len = values.len();
    go_sort::sort(&mut ByValue {
        values,
        uids,
        facets,
        desc,
        cl,
        len,
    });
    Ok(())
}

/// Dgraph's `Sort`.
pub fn sort<C: Collator>(
    values: &mut [Vec<Val>],
    uids: &mut [u64],
    desc: &[bool],
    lang: &str,
    collator: &C,
) -> Result<(), TypesError> {
    sort_with_facet(values, uids, None, desc, lang, collator)
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
        | TypeId::BIGFLOAT => Ok(less_vals::<TextCollator>(a, b, None)),
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
        (Some(Value::BigFloat(x)), Some(Value::BigFloat(y))) => x.compare(y) == Ordering::Less,
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
        | TypeId::BIGFLOAT => Ok(equal_vals(a, b)),
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
        (TypeId::BIGFLOAT, Some(Value::BigFloat(x)), Some(Value::BigFloat(y))) => {
            x.compare(y) == Ordering::Equal
        }
        _ => false,
    }
}
