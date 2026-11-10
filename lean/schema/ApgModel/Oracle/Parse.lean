/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Oracle.NQuads

/-!
# Reading N-Quads back

The parser of the oracle's export, which checks that every file it writes reads back as its quads (decision L5).

- **Syntax.** A line is a subject, a predicate, an object and a graph: IRIs in angle brackets, blank nodes `_:x`,
  literals with a datatype or a language tag, and RDF 1.2 triple terms `<<( s p o )>>` (`parseLine`).
- **Terms.** A predicate is read back through the catalog's table of the predicates it mints (`predTable`) and the
  `@iri` predicates its bases name, unique by `UGS0801`; a literal through its attribute's type in the catalog, the
  subject's label read from its type quads: a lexical form parsed back to its value (`parseLexical`), an enumeration's
  member, a JSON value (`parseJson`), a measure, a precision.
-/

namespace ApgModel.Oracle

open Named Catalog Views

/-! ## Lexical forms read back -/

/-- A natural number's decimal digits. -/
def parseNat (s : String) : Option Nat :=
  if s.isEmpty || !s.all Char.isDigit then none else some s.toNat!

/-- An integer, `-` before a negative one. -/
def parseInt (s : String) : Option Int :=
  if s.startsWith "-" then (parseNat (s.drop 1)).map fun n => -(n : Int) else (parseNat s).map Int.ofNat

/-- A decimal number scaled by `10^k`, refused when it has more fraction digits. -/
def parseScaled (s : String) (k : Nat) : Option Int :=
  let neg := s.startsWith "-"
  let body := if neg then s.drop 1 else s
  match body.splitOn "." with
  | [i] => (parseNat i).map fun n => (if neg then -1 else 1) * ((n * 10 ^ k : Nat) : Int)
  | [i, f] =>
    if f.length > k then none else
    (parseNat i).bind fun n => (parseNat f).map fun m =>
      (if neg then -1 else 1) * ((n * 10 ^ k + m * 10 ^ (k - f.length) : Nat) : Int)
  | _ => none

/-- A binary64 value read from its exact decimal, in canonical form: an odd mantissa, or zero. -/
def parseF64 (s : String) : Option F64 :=
  let neg := s.startsWith "-"
  let body := if neg then s.drop 1 else s
  let (i, f) := match body.splitOn "." with
    | [i] => (i, "")
    | [i, f] => (i, f)
    | _ => ("", "")
  (parseNat (i ++ f)).bind fun n =>
    let k := f.length
    if n % 5 ^ k ≠ 0 then none else
    let m := n / 5 ^ k
    if m = 0 then some ⟨0, 0⟩ else
    let rec norm (m : Nat) (e : Int) (fuel : Nat) : Nat × Int :=
      match fuel with
      | 0 => (m, e)
      | fuel + 1 => if m % 2 = 0 then norm (m / 2) (e + 1) fuel else (m, e)
    let (m', e') := norm m (-(k : Int)) (m + 1)
    some ⟨(if neg then -1 else 1) * (m' : Int), e'⟩

/-- A timezone suffix and the text before it: `Z`, `±hh:mm`, or none. -/
def splitTz (s : String) : String × Option Int :=
  if s.endsWith "Z" then (s.dropRight 1, some 0)
  else if s.length ≥ 6 && (s.get ⟨s.length - 6⟩ == '+' || s.get ⟨s.length - 6⟩ == '-') &&
      s.get ⟨s.length - 3⟩ == ':' then
    let tz := s.drop (s.length - 6)
    let sign : Int := if tz.startsWith "-" then -1 else 1
    match parseNat ((tz.drop 1).take 2), parseNat (tz.drop 4) with
    | some h, some m => (s.dropRight 6, some (sign * ((h * 60 + m : Nat) : Int)))
    | _, _ => (s, none)
  else (s, none)

/-- `[-]YYYY-MM-DD` as a day counted from 1970-01-01. -/
def parseDate (s : String) : Option Int :=
  let neg := s.startsWith "-"
  match (if neg then s.drop 1 else s).splitOn "-" with
  | [y, m, d] =>
    (parseNat y).bind fun y => (parseNat m).bind fun m => (parseNat d).map fun d =>
      daysOfCivil (if neg then -(y : Int) else y) m d
  | _ => none

/-- `hh:mm:ss[.f]` as nanoseconds within a day. -/
def parseClock (s : String) : Option Nat :=
  match s.splitOn ":" with
  | [h, m, sec] =>
    (parseNat h).bind fun h => (parseNat m).bind fun m => (parseScaled sec 9).map fun ns =>
      (h * 3600 + m * 60) * nanosPerSecond + ns.toNat
  | _ => none

/-- `[-]YYYY-MM-DDThh:mm:ss[.f]` as wall-clock nanoseconds from 1970-01-01T00:00:00. -/
def parseWall (s : String) : Option Int :=
  match s.splitOn "T" with
  | [d, c] => (parseDate d).bind fun d => (parseClock c).map fun n => d * nanosPerDay + n
  | _ => none

/-- A duration as months and nanoseconds. -/
def parseDuration (s : String) : Option (Int × Int) :=
  let neg := s.startsWith "-"
  let body := if neg then s.drop 1 else s
  if !body.startsWith "P" then none else
  let body := body.drop 1
  let (ym, dt) := match body.splitOn "T" with
    | [a] => (a, "")
    | [a, b] => (a, b)
    | _ => ("?", "")
  -- read `number unit` pairs in order
  let rec pairs (cs : List Char) (acc : String) (out : List (String × Char)) (fuel : Nat) :
      Option (List (String × Char)) :=
    match fuel, cs with
    | _, [] => if acc.isEmpty then some out.reverse else none
    | 0, _ => none
    | fuel + 1, c :: cs =>
      if c.isDigit || c == '.' then pairs cs (acc.push c) out fuel
      else if acc.isEmpty then none else pairs cs "" ((acc, c) :: out) fuel
  (pairs ym.toList "" [] (ym.length + 1)).bind fun ymp => (pairs dt.toList "" [] (dt.length + 1)).bind fun dtp =>
    let get (ps : List (String × Char)) (u : Char) : Option Int :=
      match ps.find? (·.2 == u) with
      | some (n, _) => parseScaled n 9
      | none => some 0
    (get ymp 'Y').bind fun y => (get ymp 'M').bind fun mo => (get ymp 'D').bind fun d =>
    (get dtp 'H').bind fun h => (get dtp 'M').bind fun mi => (get dtp 'S').map fun sec =>
      let e : Int := 1000000000
      let months := (y / e) * 12 + mo / e
      let nanos := (d / e) * nanosPerDay + (h / e) * 3600 * nanosPerSecond + (mi / e) * 60 * nanosPerSecond + sec
      if neg then (-months, -nanos) else (months, nanos)

/-- A precision from its name. -/
def parsePrecision (s : String) : Option Precision :=
  [Precision.millennium, .century, .decade, .year, .month, .day, .hour, .minute, .second, .millisecond,
    .microsecond, .nanosecond].find? (·.name == s)

/-- A primitive value read back from its lexical form and language tag. -/
def parseLexical : (p : Prim) → String → Option String → Option p.V
  | .bool, s, _ => if s == "true" then some true else if s == "false" then some false else none
  | .int _, s, _ => parseInt s
  | .decimal _ k, s, _ => parseScaled s k
  | .float64, s, _ => parseF64 s
  | .string, s, _ | .iri, s, _ | .unit, s, _ | .quantityKind, s, _ | .geo, s, _ => some s
  | .text, s, tag => some (s, tag)
  | .date, s, _ => parseDate s
  | .time, s, _ => let (c, tz) := splitTz s; (parseClock c).map fun n => ((n : Int), tz)
  | .dateTime, s, _ =>
    let (w, tz) := splitTz s
    tz.bind fun o => (parseWall w).map fun n => (n - o * 60 * nanosPerSecond, o)
  | .localDateTime, s, _ => parseWall s
  | .duration, s, _ => parseDuration s
  | .precision, s, _ => parsePrecision s

/-! ## JSON read back -/

/-- A JSON value: a string, `true` or `false`, an array or an object. -/
inductive Json where
  | str (s : String)
  | bool (b : Bool)
  | arr (vs : List Json)
  | obj (kvs : List (String × Json))

/-- A JSON string's body after its opening quote, up to its closing quote. -/
def jsonStr : List Char → String → Nat → Option (String × List Char)
  | _, _, 0 => none
  | '"' :: cs, acc, _ => some (acc, cs)
  | '\\' :: c :: cs, acc, fuel + 1 =>
    match c with
    | 'n' => jsonStr cs (acc.push '\n') fuel
    | 'r' => jsonStr cs (acc.push '\r') fuel
    | 't' => jsonStr cs (acc.push '\t') fuel
    | 'b' => jsonStr cs (acc.push '\x08') fuel
    | 'f' => jsonStr cs (acc.push '\x0c') fuel
    | 'u' =>
      let hex := cs.take 4
      let v := hex.foldl (fun n c => n * 16 + (if c.isDigit then c.toNat - 48 else c.toNat - 87)) 0
      jsonStr (cs.drop 4) (acc.push (Char.ofNat v)) fuel
    | c => jsonStr cs (acc.push c) fuel
  | c :: cs, acc, fuel + 1 => jsonStr cs (acc.push c) fuel
  | [], _, _ => none

mutual
/-- A JSON value and the text after it. -/
def jsonValue : List Char → Nat → Option (Json × List Char)
  | _, 0 => none
  | '"' :: cs, _ + 1 => (jsonStr cs "" (cs.length + 1)).map fun (s, r) => (.str s, r)
  | 't' :: 'r' :: 'u' :: 'e' :: cs, _ => some (.bool true, cs)
  | 'f' :: 'a' :: 'l' :: 's' :: 'e' :: cs, _ => some (.bool false, cs)
  | '[' :: ']' :: cs, _ => some (.arr [], cs)
  | '[' :: cs, fuel + 1 => (jsonItems cs fuel).map fun (vs, r) => (.arr vs, r)
  | '{' :: '}' :: cs, _ => some (.obj [], cs)
  | '{' :: cs, fuel + 1 => (jsonMembers cs fuel).map fun (kvs, r) => (.obj kvs, r)
  | _, _ => none
/-- An array's items up to its closing bracket. -/
def jsonItems : List Char → Nat → Option (List Json × List Char)
  | _, 0 => none
  | cs, fuel + 1 =>
    (jsonValue cs fuel).bind fun (v, r) =>
      match r with
      | ',' :: r => (jsonItems r fuel).map fun (vs, r) => (v :: vs, r)
      | ']' :: r => some ([v], r)
      | _ => none
/-- An object's members up to its closing brace. -/
def jsonMembers : List Char → Nat → Option (List (String × Json) × List Char)
  | _, 0 => none
  | '"' :: cs, fuel + 1 =>
    (jsonStr cs "" (cs.length + 1)).bind fun (k, r) =>
      match r with
      | ':' :: r =>
        (jsonValue r fuel).bind fun (v, r) =>
          match r with
          | ',' :: r => (jsonMembers r fuel).map fun (kvs, r) => ((k, v) :: kvs, r)
          | '}' :: r => some ([(k, v)], r)
          | _ => none
      | _ => none
  | _, _ => none
end

/-- A JSON value read back at its named type. -/
partial def valueOfJson : NTy typeside.P Name → Json → Option (VTerm typeside)
  | .record fs, .obj kvs =>
    (fs.mapM fun (nt : Name × NTy typeside.P Name) =>
      let (n, t) := nt
      match t, kvs.find? (fun (kv : String × Json) => kv.1 == n) with
      | .variant [("none", .record []), ("some", t')], some (_, j) =>
        (valueOfJson t' j).map fun u => (n, VTerm.inj "some" u)
      | .variant [("none", .record []), ("some", _)], none => some (n, VTerm.inj "none" (.record []))
      | _, some (_, j) => (valueOfJson t j).map fun u => (n, u)
      | _, none => none).map VTerm.record
  | .list t, .arr js => (js.mapM (valueOfJson t)).map VTerm.list
  | .set t, .arr js => (js.mapM (valueOfJson t)).map VTerm.set
  | .variant _, .str m => some (.inj m (.record []))
  | .prim .bool, .bool b => some (.prim (T := typeside) .bool b)
  | .prim p, .str s => (parseLexical p s none).map fun a => .prim (T := typeside) p a
  | _, _ => none

/-- JSON text read back at its named type. -/
def parseJson (t : NTy typeside.P Name) (s : String) : Option (VTerm typeside) :=
  (jsonValue s.toList (s.length + 1)).bind fun (j, r) => if r.isEmpty then valueOfJson t j else none

/-! ## N-Quads syntax -/

/-- A raw term of N-Quads. -/
inductive Raw where
  | iri (s : String)
  | blank (s : String)
  | lit (lex : String) (dt : String) (lang : Option String)
  | triple (s p o : Raw)

/-- Text up to a character, and the rest after it. -/
def upTo (c : Char) (cs : List Char) : Option (String × List Char) :=
  let (a, b) := cs.span (· != c)
  match b with
  | _ :: r => some (String.mk a, r)
  | [] => none

/-- A literal's lexical form after its opening quote, escapes undone. -/
def litBody : List Char → String → Nat → Option (String × List Char)
  | _, _, 0 => none
  | '"' :: cs, acc, _ => some (acc, cs)
  | '\\' :: c :: cs, acc, fuel + 1 =>
    litBody cs (acc.push (if c == 'n' then '\n' else if c == 'r' then '\r' else c)) fuel
  | c :: cs, acc, fuel + 1 => litBody cs (acc.push c) fuel
  | [], _, _ => none

/-- Blanks skipped. -/
def skipWs (cs : List Char) : List Char := cs.dropWhile (· == ' ')

/-- A raw term and the text after it. -/
def rawTerm : List Char → Nat → Option (Raw × List Char)
  | _, 0 => none
  | '<' :: '<' :: '(' :: cs, fuel + 1 =>
    (rawTerm (skipWs cs) fuel).bind fun (s, r) => (rawTerm (skipWs r) fuel).bind fun (p, r) =>
      (rawTerm (skipWs r) fuel).bind fun (o, r) =>
        match skipWs r with
        | ')' :: '>' :: '>' :: r => some (.triple s p o, r)
        | _ => none
  | '<' :: cs, _ => (upTo '>' cs).map fun (s, r) => (.iri s, r)
  | '_' :: ':' :: cs, _ => let (a, b) := cs.span (fun c => c != ' '); some (.blank ("_:" ++ String.mk a), b)
  | '"' :: cs, _ =>
    (litBody cs "" (cs.length + 1)).bind fun (lex, r) =>
      match r with
      | '^' :: '^' :: '<' :: r => (upTo '>' r).map fun (dt, r) => (.lit lex dt none, r)
      | '@' :: r => let (a, b) := r.span (· != ' '); some (.lit lex (rdfNs ++ "langString") (some (String.mk a)), b)
      | r => some (.lit lex (xsd ++ "string") none, r)
  | _, _ => none

/-- A line: subject, predicate, object, graph. -/
def parseLine (line : String) : Option (Raw × Raw × Raw × Raw) :=
  let cs := line.toList
  let n := cs.length + 1
  (rawTerm (skipWs cs) n).bind fun (s, r) => (rawTerm (skipWs r) n).bind fun (p, r) =>
    (rawTerm (skipWs r) n).bind fun (o, r) => (rawTerm (skipWs r) n).bind fun (g, r) =>
      if skipWs r == ['.'] then some (s, p, o, g) else none

/-! ## Terms read back -/

/-- The predicates a catalog mints, by name after the vocabulary base: attributes and their interval bounds,
navigation names and role predicates. -/
def predTable (K : Catalog typeside) : List (String × Pred) :=
  K.types.flatMap fun c => c.fields.flatMap fun f =>
    match f with
    | .attr a _ _ .interval =>
      [(a ++ "_start", .bound a .start), (a ++ "_end", .bound a .stop),
        (a ++ "_start_precision", .bound a .startPrecision), (a ++ "_end_precision", .bound a .stopPrecision)]
    | .attr a _ _ _ => [(a, .attr a)]
    | .role r _ _ nav _ => [(Catalog.roleLabelName c.name r, .role c.name r)] ++ nav.toList.map fun n => (n, .nav n)
    | .setRole r _ _ => [(Catalog.roleLabelName c.name r, .role c.name r)]

/-- A predicate read back from its IRI. -/
def readPred (K : Catalog typeside) (B : Bases) (iri : String) : Option Pred :=
  if iri == rdfNs ++ "type" then some .rdfType
  else if iri == rdfNs ++ "reifies" then some .reifies
  else if iri == ugdb ++ "asserted_start" then some .assertedStart
  else if iri == ugdb ++ "asserted_end" then some .assertedEnd
  else if iri == ugdb ++ "live" then some .live
  else if let some (a, _) := B.iris.find? (·.2 == iri) then some (.attr a)
  else if iri.startsWith B.vocab then
    ((predTable K).find? (·.1 == iri.drop B.vocab.length)).map Prod.snd
  else none

/-- A node read back: an IRI or a blank node. -/
def readNode : Raw → Option String
  | .iri s => some s
  | .blank s => some s
  | _ => none

/-- The slot an attribute or bound predicate reads, at the label `l`. -/
def slotOf (K : Catalog typeside) (l a : Name) : Option (Bool × NTy typeside.P Name × AttrView) :=
  (K.slots l).findSome? fun s =>
    match s with
    | (n, .attr opt t view) => if n == a then some (opt, t, view) else none
    | _ => none

/-- A literal of a primitive type read back. -/
def readLit (p : Prim) (lex : String) (lang : Option String) : Option (Term typeside) :=
  (parseLexical p lex lang).map fun a => .lit (T := typeside) p a

/-- An object read back, the subject's label `l` giving its attribute's type. -/
def readObject (K : Catalog typeside) (B : Bases) (l : Name) (p : Pred) (o : Raw) : Option (Term typeside) :=
  match p, o with
  | .rdfType, .iri c => if c.startsWith B.vocab then some (.cls (c.drop B.vocab.length)) else none
  | .reifies, .triple s (.iri n) t =>
    (readNode s).bind fun s => (readNode t).map fun t => .triple s (n.drop B.vocab.length) t
  | .live, .lit lex _ _ =>
    if lex == "true" then some (.bool true) else if lex == "false" then some (.bool false) else none
  | .assertedStart, .lit lex _ lang | .assertedEnd, .lit lex _ lang => readLit .dateTime lex lang
  | .nav _, o | .role _ _, o => (readNode o).map .iri
  | .bound a b, .lit lex _ lang =>
    (slotOf K l a).bind fun (_, t, _) =>
      let fieldOf (n : Name) : Option (NTy typeside.P Name) := match t with
        | .record fs => (fs.find? (·.1 == n)).map Prod.snd
        | _ => none
      let name := match b with
        | .start => "start" | .stop => "end" | .startPrecision => "start_precision" | .stopPrecision => "end_precision"
      match fieldOf name with
      | some (.variant [_, (_, .prim q)]) => readLit q lex lang
      | _ => none
  | .attr a, o =>
    (slotOf K l a).bind fun (_, t, view) =>
      match view, o with
      | .typeValue, .iri c => if c.startsWith B.vocab then some (.cls (c.drop B.vocab.length)) else none
      | .measure, .lit lex _ _ =>
        match lex.splitOn " " with
        | [num, unit] => (parseF64 num).map fun x =>
            .measure (.record [("unit", .prim (T := typeside) .unit unit), ("value", .prim (T := typeside) .float64 x)])
        | _ => none
      | _, .lit lex dt lang =>
        if dt == rdfNs ++ "JSON" then (parseJson t lex).map Term.json else
        let member := match t with
          | .set m => m
          | m => m
        match member with
        | .prim q => readLit q lex lang
        | .variant _ => some (.member lex)
        | _ => none
      | _, _ => none
  | _, _ => none

/-- An N-Quads document read back as the view's quads, each subject's label read from its type quads. -/
def readNQuads (K : Catalog typeside) (B : Bases) (lines : List String) : Option (List (Quad typeside)) :=
  (lines.mapM parseLine).bind fun raws =>
    let label (s : String) : Name :=
      let cs := raws.filterMap fun (s', p, o, _) =>
        if readNode s' == some s && (match p with | .iri i => i == rdfNs ++ "type" | _ => false) then
          match o with
          | .iri c => some (c.drop B.vocab.length)
          | _ => none
        else none
      (cs.find? fun c => cs.all fun c' => (K.chain c).contains c').getD ""
    raws.mapM fun (s, p, o, g) =>
      (readNode s).bind fun s => (match p with | .iri i => readPred K B i | _ => none).bind fun p =>
        let l := match p with
          | .nav _ | .role _ _ => ""
          | _ => label s
        (readObject K B l p o).bind fun o => (readNode g).map fun g => ⟨s, p, o, g⟩

end ApgModel.Oracle
