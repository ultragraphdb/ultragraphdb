/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Oracle.Typeside
import ApgModel.Named.Apg

/-!
# Lexical forms

The canonical lexical forms of `PROTOCOL.md` section 10.5 for the oracle's typeside, the datatype each primitive type is
stored and exported under (`SCHEMA.md` section 5.1), and the JSON of an `rdf:JSON` literal (`SCHEMA.md` section 11.2):
RFC 8785's canonical form, a record an object of its present fields, a list an array, an enum member a string, a `Bool`
`true` or `false`, every other scalar a string of its lexical form.

- The calendar is the proleptic Gregorian with astronomical year numbering, days counted from 1970-01-01
  (`civilOfDays`, `daysOfCivil`, Hinnant's algorithms over the integers).
- A `Float64` is written as its exact decimal. Every binary64 value has one; it is the shortest decimal that reads back
  as the value, as `PROTOCOL.md` asks, for the values the generator writes, whose mantissas and exponents are small
  (`Oracle.Generate`).
- A language-tagged `Text` inside a record or list has no JSON form the specification gives; the generator writes none.
-/

namespace ApgModel.Oracle

open Named

/-! ## Digits and the calendar -/

/-- A natural number's decimal digits, zero-padded to `w`. -/
def pad (w n : Nat) : String :=
  let s := toString n
  String.mk (List.replicate (w - s.length) '0') ++ s

/-- The date of a day counted from 1970-01-01: year, month, day. -/
def civilOfDays (z : Int) : Int × Nat × Nat :=
  let z := z + 719468
  let era := z / 146097
  let doe := z - era * 146097
  let yoe := (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365
  let doy := doe - (365 * yoe + yoe / 4 - yoe / 100)
  let mp := (5 * doy + 2) / 153
  let d := doy - (153 * mp + 2) / 5 + 1
  let m := if mp < 10 then mp + 3 else mp - 9
  let y := yoe + era * 400 + (if m ≤ 2 then 1 else 0)
  (y, m.toNat, d.toNat)

/-- The day counted from 1970-01-01 of a date. -/
def daysOfCivil (y : Int) (m d : Nat) : Int :=
  let y := if m ≤ 2 then y - 1 else y
  let era := y / 400
  let yoe := y - era * 400
  let mp : Int := if m > 2 then m - 3 else m + 9
  let doy := (153 * mp + 2) / 5 + d - 1
  let doe := yoe * 365 + yoe / 4 - yoe / 100 + doy
  era * 146097 + doe - 719468

/-- Nanoseconds in a second and in a day. -/
def nanosPerSecond : Nat := 1000000000
def nanosPerDay : Nat := 86400 * nanosPerSecond

/-- A fraction of a second, `.f` without trailing zeros, nothing when zero. -/
def fracText (nanos : Nat) : String :=
  if nanos = 0 then "" else "." ++ (pad 9 nanos).dropRightWhile (· == '0')

/-- A timezone offset in minutes: `Z`, else `±hh:mm`. -/
def tzText (o : Int) : String :=
  if o = 0 then "Z" else (if o < 0 then "-" else "+") ++ pad 2 (o.natAbs / 60) ++ ":" ++ pad 2 (o.natAbs % 60)

/-- A year, at least four digits, `-` before a negative one. -/
def yearText (y : Int) : String := (if y < 0 then "-" else "") ++ pad 4 y.natAbs

/-- `[-]YYYY-MM-DD`. -/
def dateText (days : Int) : String :=
  let (y, m, d) := civilOfDays days
  yearText y ++ "-" ++ pad 2 m ++ "-" ++ pad 2 d

/-- `hh:mm:ss[.f]` of nanoseconds within a day. -/
def clockText (n : Nat) : String :=
  let s := n / nanosPerSecond
  pad 2 (s / 3600) ++ ":" ++ pad 2 (s % 3600 / 60) ++ ":" ++ pad 2 (s % 60) ++ fracText (n % nanosPerSecond)

/-- `[-]YYYY-MM-DDThh:mm:ss[.f]` of wall-clock nanoseconds from 1970-01-01T00:00:00. -/
def wallText (n : Int) : String :=
  dateText (n / nanosPerDay) ++ "T" ++ clockText (n % nanosPerDay).toNat

/-- A decimal number scaled by `10^s`: the integer part, then `.` and the fraction without trailing zeros when
nonzero. -/
def scaledText (x : Int) (s : Nat) : String :=
  let a := x.natAbs
  let i := a / 10 ^ s
  let f := a % 10 ^ s
  (if x < 0 then "-" else "") ++ toString i ++ (if f = 0 then "" else "." ++ (pad s f).dropRightWhile (· == '0'))

/-- A binary64 value `m · 2^e` as its exact decimal. -/
def f64Text (x : F64) : String :=
  if x.exponent ≥ 0 then toString (x.mantissa * 2 ^ x.exponent.toNat)
  else scaledText (x.mantissa * 5 ^ (-x.exponent).toNat) (-x.exponent).toNat

/-- A duration of months and nanoseconds, one sign: `-` when negative, then `P`, the year-month part and the day-time
part, each only when nonzero; `PT0S` when zero. -/
def durationText (months nanos : Int) : String :=
  if months = 0 ∧ nanos = 0 then "PT0S" else
  let neg := months < 0 ∨ nanos < 0
  let mo := months.natAbs
  let n := nanos.natAbs
  let days := n / nanosPerDay
  let rest := n % nanosPerDay
  let secs := rest / nanosPerSecond
  let h := secs / 3600
  let mi := secs % 3600 / 60
  let sn := rest % (60 * nanosPerSecond)
  let part (k : Nat) (c : String) : String := if k = 0 then "" else toString k ++ c
  let dayTime := part h "H" ++ part mi "M" ++ (if sn = 0 then "" else scaledText sn 9 ++ "S")
  (if neg then "-" else "") ++ "P" ++ part (mo / 12) "Y" ++ part (mo % 12) "M" ++ part days "D" ++
    (if dayTime = "" then "" else "T" ++ dayTime)

/-- A precision's name. -/
def Precision.name : Precision → String
  | .millennium => "millennium" | .century => "century" | .decade => "decade" | .year => "year"
  | .month => "month" | .day => "day" | .hour => "hour" | .minute => "minute" | .second => "second"
  | .millisecond => "millisecond" | .microsecond => "microsecond" | .nanosecond => "nanosecond"

/-! ## Datatypes and lexical forms -/

def xsd : String := "http://www.w3.org/2001/XMLSchema#"
def rdfNs : String := "http://www.w3.org/1999/02/22-rdf-syntax-ns#"
def ugdb : String := "https://w3id.org/ultragraphdb/ns/"
def geoNs : String := "http://www.opengis.net/ont/geosparql#"

/-- The datatype of an integer width. -/
def Width.datatype : Width → String
  | .i8 => xsd ++ "byte" | .i16 => xsd ++ "short" | .i32 => xsd ++ "int" | .i64 => xsd ++ "long"
  | .i128 => xsd ++ "integer" | .u8 => xsd ++ "unsignedByte" | .u16 => xsd ++ "unsignedShort"
  | .u32 => xsd ++ "unsignedInt" | .u64 => xsd ++ "unsignedLong"

/-- The datatype a primitive type is exported under (`SCHEMA.md` section 5.1); `Text` with a tag is `rdf:langString`. -/
def Prim.datatype : Prim → String
  | .bool => xsd ++ "boolean"
  | .int w => w.datatype
  | .decimal _ _ => xsd ++ "decimal"
  | .float64 => xsd ++ "double"
  | .string | .text | .unit | .quantityKind => xsd ++ "string"
  | .date => xsd ++ "date"
  | .time => xsd ++ "time"
  | .dateTime => xsd ++ "dateTimeStamp"
  | .localDateTime => xsd ++ "dateTime"
  | .duration => xsd ++ "duration"
  | .precision => ugdb ++ "precision"
  | .iri => xsd ++ "anyURI"
  | .geo => geoNs ++ "geoJSONLiteral"

/-- A value's canonical lexical form (`PROTOCOL.md` section 10.5). -/
def Prim.lexical : (p : Prim) → p.V → String
  | .bool, b => if b then "true" else "false"
  | .int _, x => toString x
  | .decimal _ s, x => scaledText x s
  | .float64, x => f64Text x
  | .string, x | .iri, x | .unit, x | .quantityKind, x | .geo, x => x
  | .text, (x, _) => x
  | .date, d => dateText d
  | .time, (n, o) => clockText n.toNat ++ (match o with | some o => tzText o | none => "")
  | .dateTime, (n, o) => wallText (n + o * 60 * nanosPerSecond) ++ tzText o
  | .localDateTime, n => wallText n
  | .duration, (m, n) => durationText m n
  | .precision, p => p.name

/-- A `Text` value's language tag. -/
def Prim.language : (p : Prim) → p.V → Option String
  | .text, (_, tag) => tag
  | _, _ => none

/-! ## JSON -/

/-- A string as a JSON string (RFC 8785): `"` and `\` escaped, control characters as `\b`, `\f`, `\n`, `\r`, `\t` or
`\u00hh`. -/
def jsonString (s : String) : String :=
  let hex (n : Nat) : Char := if n < 10 then Char.ofNat (48 + n) else Char.ofNat (87 + n)
  "\"" ++ String.join (s.toList.map fun c =>
    if c = '"' then "\\\"" else if c = '\\' then "\\\\"
    else if c = '\n' then "\\n" else if c = '\r' then "\\r" else if c = '\t' then "\\t"
    else if c = '\x08' then "\\b" else if c = '\x0c' then "\\f"
    else if c.toNat < 32 then "\\u00" ++ String.mk [hex (c.toNat / 16), hex (c.toNat % 16)]
    else c.toString) ++ "\""

/-- Insertion of a keyed entry into a list sorted by key. -/
def insertKey {α : Type} (x : String × α) : List (String × α) → List (String × α)
  | [] => [x]
  | y :: ys => if y.1 < x.1 then y :: insertKey x ys else x :: y :: ys

/-- A value's JSON at its named type: a record an object of its present fields sorted by name, an optional field's
absence left out, a list or set an array, an enumeration's member a string, a `Bool` `true` or `false`, any other
primitive a string of its lexical form. -/
partial def jsonOf : NTy typeside.P Name → VTerm typeside → String
  | .record fs, .record ws =>
    let entries := fs.filterMap fun (n, t) =>
      match ws.find? (·.1 == n) with
      | some (_, w) =>
        match t, w with
        | .variant [("none", .record []), ("some", t')], .inj "some" u => some (n, jsonOf t' u)
        | .variant [("none", .record []), ("some", _)], _ => none
        | _, _ => some (n, jsonOf t w)
      | none => none
    "{" ++ ",".intercalate ((entries.foldr insertKey []).map fun (k, v) => jsonString k ++ ":" ++ v) ++ "}"
  | .list t, .list vs | .set t, .set vs => "[" ++ ",".intercalate (vs.map (jsonOf t)) ++ "]"
  | .variant _, .inj m _ => jsonString m
  | _, .prim .bool b => Prim.lexical .bool b
  | _, .prim p a => jsonString (Prim.lexical p a)
  | _, _ => "null"

end ApgModel.Oracle
