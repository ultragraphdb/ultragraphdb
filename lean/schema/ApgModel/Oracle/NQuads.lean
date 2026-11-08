/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Oracle.Lexical
import ApgModel.Views.RdfInverse

/-!
# N-Quads

The oracle's export of `R(G)` (`SCHEMA-APG.md` section 19, decision L5): the view's structured quads written as N-Quads.

- **Names.** A keyed element is named by the IRI its key mints, `{base}{KeySpace}/{key}`, each component's canonical
  lexical form percent-encoded outside RFC 3986's unreserved characters (`SCHEMA.md` sections 7.2 and 7.3); an element
  whose IRI the engine draws, a handle or a reifier, by a blank node `_:e{n}`, as a write names it; an element named by
  its triple or pair by none (`naming`).
- **Vocabulary.** A class or a predicate is `{base}` and its name as `SCHEMA.md` section 7.4 gives it; `rdf:type`,
  `rdf:reifies` and the built-in fields' `ugdb:` predicates are fixed.
- **Literals.** A literal is its canonical lexical form under its datatype, an `xsd:string` written plain and a tagged
  `Text` as `rdf:langString`; an `rdf:JSON` literal is the JSON of its value at its attribute's type, read from the
  catalog; a measure is `{number} {unit}` under `ugdb:measure`.
-/

namespace ApgModel.Oracle

open Named Catalog Views

/-- What an export names its terms under: the vocabulary base, the instance base and the graph. -/
structure Bases where
  /-- The namespace's vocabulary base. -/
  vocab : String
  /-- The instance base of the graph. -/
  inst : String
  /-- The graph's IRI. -/
  graph : String

/-! ## Names -/

/-- A string percent-encoded outside RFC 3986's unreserved characters, byte by byte in UTF-8, uppercase hex. -/
def percentEncode (s : String) : String :=
  let hex (n : Nat) : Char := if n < 10 then Char.ofNat (48 + n) else Char.ofNat (55 + n)
  String.join (s.toUTF8.toList.map fun b =>
    let c := Char.ofNat b.toNat
    if c.isAlphanum || c = '-' || c = '.' || c = '_' || c = '~' then c.toString
    else "%" ++ String.mk [hex (b.toNat / 16), hex (b.toNat % 16)])

/-- A key component's canonical lexical form (`SCHEMA.md` section 7.2): a `DateTime` in UTC, an enum member its
name. -/
def keyComponent : VTerm typeside → String
  | .prim .dateTime (n, _) => Prim.lexical .dateTime (n, 0)
  | .prim p a => Prim.lexical p a
  | .inj m _ => m
  | _ => ""

/-- The type that first declares the key of `l`: its key space. -/
def keySpace (K : Catalog typeside) (l : Name) : Name :=
  ((K.chain l).reverse.find? fun m => ((K.find m).map fun c => !c.key.isEmpty).getD false).getD l

/-- The naming of an export: a keyed element by its key's IRI, an element the engine names by a blank node, the
others by none. -/
def naming (K : Catalog typeside) (B : Bases) (G : FinApg K.schema) (e : Nat) : Option String :=
  match G.entry e with
  | some (l, v) =>
    match K.find l with
    | some _ =>
      if tripleNamed K l then none
      else
        let ks := K.keyOf l
        if ks.isEmpty then some ("_:e" ++ toString e)
        else some (B.inst ++ keySpace K l ++ "/" ++
          "/".intercalate (ks.map fun k => percentEncode (((v.field? k).map keyComponent).getD "")))
    | none => none
  | none => none

/-! ## Terms -/

/-- A node: a blank node as written, an IRI in angle brackets. -/
def nodeText (s : String) : String := if s.startsWith "_:" then s else "<" ++ s ++ ">"

/-- An N-Quads string literal's body, `"`, `\`, line feed and carriage return escaped. -/
def literalBody (s : String) : String :=
  "\"" ++ String.join (s.toList.map fun c =>
    if c = '"' then "\\\"" else if c = '\\' then "\\\\" else if c = '\n' then "\\n"
    else if c = '\r' then "\\r" else c.toString) ++ "\""

/-- A literal of a lexical form and a datatype, `xsd:string` written plain. -/
def literalText (lex dt : String) : String :=
  if dt = xsd ++ "string" then literalBody lex else literalBody lex ++ "^^<" ++ dt ++ ">"

/-- A predicate's IRI. -/
def predIri (B : Bases) : Pred → String
  | .rdfType => rdfNs ++ "type"
  | .reifies => rdfNs ++ "reifies"
  | .attr a => B.vocab ++ a
  | .bound a .start => B.vocab ++ a ++ "_start"
  | .bound a .stop => B.vocab ++ a ++ "_end"
  | .bound a .startPrecision => B.vocab ++ a ++ "_start_precision"
  | .bound a .stopPrecision => B.vocab ++ a ++ "_end_precision"
  | .assertedStart => ugdb ++ "asserted_start"
  | .assertedEnd => ugdb ++ "asserted_end"
  | .live => ugdb ++ "live"
  | .nav n => B.vocab ++ n
  | .role R r => B.vocab ++ Catalog.roleLabelName R r

/-- The base type of the attribute `a` of the label `l`, read from the catalog. -/
def attrType (K : Catalog typeside) (l a : Name) : NTy typeside.P Name :=
  ((K.slots l).findSome? fun s =>
    match s with
    | (n, .attr _ t _) => if n == a then some t else none
    | _ => none).getD (.record [])

/-- An object, the label `l` of the quad's element giving an `rdf:JSON` literal's type. -/
def termText (K : Catalog typeside) (B : Bases) (l : Name) (p : Pred) : Term typeside → String
  | .iri s => nodeText s
  | .cls c => "<" ++ B.vocab ++ c ++ ">"
  | .lit q a =>
    match Prim.language q a with
    | some tag => literalBody (Prim.lexical q a) ++ "@" ++ tag
    | none => literalText (Prim.lexical q a) (Prim.datatype q)
  | .member m => literalBody m
  | .bool b => literalText (if b then "true" else "false") (xsd ++ "boolean")
  | .json v =>
    let a := match p with
      | .attr a => a
      | _ => ""
    literalText (jsonOf (attrType K l a) v) (rdfNs ++ "JSON")
  | .measure v =>
    let num := match v.field? "value" with
      | some (.prim .float64 x) => f64Text x
      | _ => ""
    let unit := match v.field? "unit" with
      | some (.prim .unit u) => u
      | _ => ""
    literalText (num ++ " " ++ unit) (ugdb ++ "measure")
  | .triple s n o => "<<( " ++ nodeText s ++ " <" ++ B.vocab ++ n ++ "> " ++ nodeText o ++ " )>>"

/-- One quad of the element of label `l` as an N-Quads line. -/
def quadText (K : Catalog typeside) (B : Bases) (l : Name) (q : Quad typeside) : String :=
  nodeText q.s ++ " <" ++ predIri B q.p ++ "> " ++ termText K B l q.p q.o ++ " <" ++ q.g ++ "> ."

/-- `R(G)` as N-Quads, one line per quad, each element's quads together. -/
def exportView (K : Catalog typeside) (vp : ViewParams typeside) (B : Bases) (G : FinApg K.schema) : List String :=
  G.elems.flatMap fun x =>
    ((templates K vp x.1 x.2.1 x.2.2).filterMap (Tmpl.inst (naming K B G) fun _ => B.graph)).map
      (quadText K B x.2.1)

end ApgModel.Oracle
