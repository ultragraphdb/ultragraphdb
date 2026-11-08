/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Typeside

/-!
# The oracle's typeside

Decision L6 of `openspec/notes/archive/prepare-apg-lean.md`: the primitive types Gen5 and `SCHEMA.md` section 16's
aviation example write, with `SCHEMA.md` section 5.1's values, each with decidable equality and a decidable linear
order, which a set's canonical order needs.

- The integers carry their width, and a value is an `Int` that `Prim.admits` checks against the width's range;
  `Decimal(p, s)` is an `Int` scaled by `10^s`.
- `Float64` is the finite values of binary64 as a mantissa and a binary exponent, `m · 2^e`, in the canonical form
  `Prim.admits` checks: an odd mantissa, or zero with exponent zero. Lean's `Float` has no decidable equality.
- `Text` is a string with an optional language tag.
- The temporal types are integers: `Date` days since 1970-01-01; `Time` nanoseconds of the day with an optional
  offset in minutes; `DateTime` nanoseconds since the epoch in UTC with its offset in minutes, compared in UTC;
  `LocalDateTime` nanoseconds since the epoch as wall-clock time; `Duration` months and nanoseconds.
- `Precision` is section 5.5's twelve precisions; `Iri`, `Unit` and `QuantityKind` are names, and `Geo` its GeoJSON
  text.

`Vector` waits for stage 8, `Bytes` and `Float32` for a schema of the corpus writing them. The typeside declares no
functions.
-/

namespace ApgModel.Oracle

/-- The integer widths, signed and unsigned. -/
inductive Width where
  | i8 | i16 | i32 | i64 | i128
  | u8 | u16 | u32 | u64
  deriving DecidableEq, Repr

/-- A width's least and greatest values. -/
def Width.bounds : Width → Int × Int
  | .i8 => (-2 ^ 7, 2 ^ 7 - 1)
  | .i16 => (-2 ^ 15, 2 ^ 15 - 1)
  | .i32 => (-2 ^ 31, 2 ^ 31 - 1)
  | .i64 => (-2 ^ 63, 2 ^ 63 - 1)
  | .i128 => (-2 ^ 127, 2 ^ 127 - 1)
  | .u8 => (0, 2 ^ 8 - 1)
  | .u16 => (0, 2 ^ 16 - 1)
  | .u32 => (0, 2 ^ 32 - 1)
  | .u64 => (0, 2 ^ 64 - 1)

/-- Section 5.5's precisions, coarsest first. -/
inductive Precision where
  | millennium | century | decade | year | month | day | hour | minute | second | millisecond | microsecond | nanosecond
  deriving DecidableEq, Repr

/-- A precision's rank, coarsest first. -/
def Precision.rank : Precision → Nat
  | .millennium => 0 | .century => 1 | .decade => 2 | .year => 3 | .month => 4 | .day => 5 | .hour => 6
  | .minute => 7 | .second => 8 | .millisecond => 9 | .microsecond => 10 | .nanosecond => 11

/-- The primitive types. -/
inductive Prim where
  | bool
  | int (w : Width)
  | decimal (p s : Nat)
  | float64
  | string
  | text
  | date
  | time
  | dateTime
  | localDateTime
  | duration
  | precision
  | iri
  | unit
  | quantityKind
  | geo
  deriving DecidableEq, Repr

/-- A binary64 value `mantissa · 2^exponent`. -/
structure F64 where
  /-- The mantissa. -/
  mantissa : Int
  /-- The binary exponent. -/
  exponent : Int
  deriving DecidableEq, Repr

/-- A primitive type's values. -/
@[reducible]
def Prim.V : Prim → Type
  | .bool => Bool
  | .int _ | .decimal _ _ | .date | .localDateTime => Int
  | .float64 => F64
  | .string | .iri | .unit | .quantityKind | .geo => String
  | .text => String × Option String
  | .time => Int × Option Int
  | .dateTime => Int × Int
  | .duration => Int × Int
  | .precision => Precision

instance (p : Prim) : DecidableEq p.V := by
  cases p <;> simp only [Prim.V] <;> exact inferInstance

/-- Whether a value lies in its type's values: an integer in its width's range, a decimal within its digits, a float
in canonical form, a time of day within the day and an offset within ±18 hours. -/
def Prim.admits : (p : Prim) → p.V → Bool
  | .int w, x => decide (w.bounds.1 ≤ x ∧ x ≤ w.bounds.2)
  | .decimal p _, x => decide (x.natAbs < 10 ^ p)
  | .float64, x => (x.mantissa % 2 != 0) || (x.mantissa == 0 && x.exponent == 0)
  | .time, (t, o) => decide (0 ≤ t ∧ t < 86400 * 10 ^ 9) && o.all fun o => decide (-1080 ≤ o ∧ o ≤ 1080)
  | .dateTime, (_, o) => decide (-1080 ≤ o ∧ o ≤ 1080)
  | _, _ => true

/-- Lexicographic order of two comparisons. -/
def thenCmp (a b : Ordering) : Ordering := if a == .eq then b else a

/-- An option ordered with `none` first. -/
def cmpOption {α : Type} (c : α → α → Ordering) : Option α → Option α → Ordering
  | none, none => .eq
  | none, some _ => .lt
  | some _, none => .gt
  | some a, some b => c a b

/-- The linear order of each primitive type's values: the integers by value, a float by its value, a pair
lexicographically, `DateTime` in UTC first. -/
def Prim.cmp : (p : Prim) → p.V → p.V → Ordering
  | .bool, a, b => compare a.toNat b.toNat
  | .int _, a, b | .decimal _ _, a, b | .date, a, b | .localDateTime, a, b => compare a b
  | .float64, a, b =>
    let e := min a.exponent b.exponent
    compare (a.mantissa * 2 ^ (a.exponent - e).toNat) (b.mantissa * 2 ^ (b.exponent - e).toNat)
  | .string, a, b | .iri, a, b | .unit, a, b | .quantityKind, a, b | .geo, a, b => compare a b
  | .text, (a, l), (b, m) => thenCmp (compare a b) (cmpOption compare l m)
  | .time, (a, o), (b, q) => thenCmp (compare a b) (cmpOption compare o q)
  | .dateTime, (a, o), (b, q) => thenCmp (compare a b) (compare o q)
  | .duration, (a, s), (b, t) => thenCmp (compare a b) (compare s t)
  | .precision, a, b => compare a.rank b.rank

/-- The oracle's typeside: no functions. -/
def typeside : Typeside where
  P := Prim
  PV := Prim.V
  F := Empty
  dom := fun f => nomatch f
  cod := fun f => nomatch f
  interp := fun f => nomatch f

instance : DecidableEq typeside.P := inferInstanceAs (DecidableEq Prim)

instance (p : typeside.P) : DecidableEq (typeside.PV p) := inferInstanceAs (DecidableEq (Prim.V p))

end ApgModel.Oracle
