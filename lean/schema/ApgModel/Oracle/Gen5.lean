/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Oracle.Aviation

/-!
# A Gen5 subset

A subset of `specification/schema/deepbrain-gen5.ugs` as a catalog over the oracle's typeside (decision L8), holding
every relation shape and every attribute view the aviation schema lacks, and a generator of its valid instance sets.

- The statement envelope: `Statement` `@bitemporal` with `valid`, an `Interval<DateTime>`, and `uncertainty`, the
  `Uncertainty` record written as JSON; `Asserted`'s method, confidence, `inferred` and `rank`.
- `Node` keyed by `id`, its IRI `{base}Node/{id}`; agents, an object, an event with a measure and a fixed-unit
  measure, a quantity, kinds with `ProposedKind`'s catalog type as a value, a teloid with a `List<KeyValue>` and
  `action_identifier` under the predicate its `@iri` names.
- `MemberOf`, node-encoded with three roles, one optional, specializing `Relation`'s, with
  `@on_retract(collection: restrict open)`; `ConnectsTo`, `@symmetric`;
  `DerivesFrom` and `ClassifiedAs`, edge-encoded with reifiers; `Alternatives`, its set-valued role `members` reified as
  the role label `Alternatives_members`; `AssertedBy`, edge-encoded without a reifier.

Every type the sets write carries every required attribute and role its declaration in the source states. Left out:
`embedding` (`Vector`, stage 8), `Gap`, `Source`, `Absence` (a role naming a catalog type), and the optional attributes
the subset's sets leave absent.

The sets are states the engine reaches: retracting the object `OBJ:o1` retracts, by `ConnectsTo`'s default
`@on_retract(cascade)`, the connection naming it in the same commit (`SCHEMA.md` section 8.4, rule 6), so the live slice
stays closed under references (`SCHEMA-APG.md`, proposition 15.2); and every interval bound coarser than its type is its
unit's first instant (`SCHEMA.md` section 12.2).
-/

namespace ApgModel.Oracle.Gen5

open Named Catalog Views
open ApgModel.Oracle.Aviation (prim present nativePrecision viewParams ord)

/-- `Interval<DateTime>`. -/
def instantInterval : NTy typeside.P Name :=
  .record [("end", NTy.optional (.prim Prim.dateTime)), ("end_precision", NTy.optional (.prim Prim.precision)),
    ("start", NTy.optional (.prim Prim.dateTime)), ("start_precision", NTy.optional (.prim Prim.precision))]

/-- A measure with its unit on the value (`SCHEMA-APG.md` section 13.2). -/
def measure : NTy typeside.P Name := .record [("unit", .prim Prim.unit), ("value", .prim Prim.float64)]

/-- GUM's `Uncertainty`. -/
def uncertainty : NTy typeside.P Name :=
  .record [("coverage_factor", NTy.optional (.prim Prim.float64)),
    ("coverage_probability", NTy.optional (.prim Prim.float64)),
    ("kind", NTy.enum ["coefficient_of_variation", "expanded", "standard", "variance"]),
    ("unit", .prim Prim.unit), ("value", .prim Prim.float64)]

/-- `KeyValue`. -/
def keyValue : NTy typeside.P Name := .record [("key", .prim Prim.string), ("value", .prim Prim.string)]

/-- An optional attribute of the plain view. -/
def opt (a : Name) (t : NTy typeside.P Name) : Field typeside := .attr a t true .plain

/-- A required attribute of the plain view. -/
def req (a : Name) (t : NTy typeside.P Name) : Field typeside := .attr a t false .plain

/-- The subset's catalog, prefixes in declaration order from 3. -/
def catalog : Catalog typeside where
  dateTime := Prim.dateTime
  types := [
    { name := "Statement", kind := .statement, parent := none, abstract := true, pfx := 3, bitemporal := true,
      fields := [.attr "valid" instantInterval true .interval, opt "uncertainty" uncertainty] },
    { name := "Asserted", kind := .statement, parent := some "Statement", abstract := true, pfx := 4,
      fields := [
        opt "method" (NTy.enum ["adlr", "conversion", "import", "llm", "manual", "nlp", "observation", "replay",
          "rule"]),
        opt "confidence" (.prim Prim.float64), req "inferred" (.prim Prim.bool),
        req "rank" (NTy.enum ["deprecated", "normal", "preferred"])] },
    { name := "Node", kind := .entity, parent := some "Asserted", abstract := true, pfx := 5, key := ["id"],
      fields := [req "id" (.prim Prim.string), opt "label" (.prim Prim.text), opt "description" (.prim Prim.text),
        opt "external_iri" (.prim Prim.iri), opt "contextoid_id" (.prim (Prim.int .u64))] },
    { name := "Endurant", kind := .entity, parent := some "Node", abstract := true, pfx := 6, fields := [] },
    { name := "Object", kind := .entity, parent := some "Endurant", abstract := false, pfx := 7,
      fields := [opt "summary" (.prim Prim.text), opt "uri" (.prim Prim.iri), opt "source_key" (.prim Prim.string),
        opt "level" (.prim (Prim.int .u8))] },
    { name := "Agent", kind := .entity, parent := some "Endurant", abstract := true, pfx := 8, fields := [] },
    { name := "Person", kind := .entity, parent := some "Agent", abstract := false, pfx := 9,
      fields := [opt "personal_name" (.prim Prim.text), opt "personal_identifiers" (.set (.prim Prim.string)),
        opt "personal_description" (.prim Prim.text)] },
    { name := "Organization", kind := .entity, parent := some "Agent", abstract := false, pfx := 10,
      fields := [opt "identifiers" (.set (.prim Prim.string))] },
    { name := "Company", kind := .entity, parent := some "Organization", abstract := false, pfx := 11, fields := [] },
    { name := "Perdurant", kind := .entity, parent := some "Node", abstract := true, pfx := 12,
      fields := [opt "summary" (.prim Prim.text), opt "where" (.prim Prim.geo),
        opt "where_uncertainty" (.prim Prim.float64), .attr "altitude" measure true .measure] },
    { name := "Event", kind := .entity, parent := some "Perdurant", abstract := false, pfx := 13,
      fields := [opt "uri" (.prim Prim.iri), opt "source_key" (.prim Prim.string)] },
    { name := "Information", kind := .entity, parent := some "Node", abstract := true, pfx := 14,
      fields := [opt "uri" (.prim Prim.iri), opt "source_key" (.prim Prim.string)] },
    { name := "Content", kind := .entity, parent := some "Information", abstract := true, pfx := 15, fields := [] },
    { name := "Quantity", kind := .entity, parent := some "Content", abstract := false, pfx := 16,
      fields := [.attr "value" measure true .measure, opt "text" (.prim Prim.string),
        req "quantity_kind" (.prim Prim.quantityKind)] },
    { name := "Meaning", kind := .entity, parent := some "Node", abstract := true, pfx := 17,
      fields := [opt "alt_labels" (.set (.prim Prim.text))] },
    { name := "Kind", kind := .entity, parent := some "Meaning", abstract := true, pfx := 18, fields := [] },
    { name := "RoleKind", kind := .entity, parent := some "Kind", abstract := false, pfx := 19, fields := [] },
    { name := "ProposedKind", kind := .entity, parent := some "Kind", abstract := false, pfx := 20,
      fields := [req "type_name" (.prim Prim.string),
        .attr "supertype" (NTy.enum ["Company", "Event", "Object", "Organization", "Person", "ProposedKind",
          "Quantity", "RoleKind", "Teloid"]) false .typeValue] },
    { name := "Purpose", kind := .entity, parent := some "Content", abstract := true, pfx := 21, fields := [] },
    { name := "Directive", kind := .entity, parent := some "Purpose", abstract := true, pfx := 22,
      fields := [opt "statement" (.prim Prim.text),
        req "state" (NTy.enum ["active", "draft", "retired", "superseded", "validated"]),
        req "version" (.prim (Prim.int .u32)), .attr "efficacious" instantInterval true .interval] },
    { name := "Teloid", kind := .entity, parent := some "Directive", abstract := false, pfx := 23,
      fields := [req "teloid_id" (.prim (Prim.int .u64)), req "action_identifier" (.prim Prim.string),
        req "modality" (NTy.enum ["impermissible", "obligatory", "optional", "permitted", "right"]),
        req "timestamp" (.prim (Prim.int .i64)), req "specificity" (.prim (Prim.int .i32)),
        req "priority" (.prim (Prim.int .i32)), opt "tags" (.set (.prim Prim.string)),
        opt "meta" (.list keyValue), req "predicate_kind" (NTy.enum ["deterministic", "uncertain"]),
        req "predicate_id" (.prim Prim.string)] },
    { name := "Relation", kind := .relation, parent := some "Asserted", abstract := true, pfx := 24, multi := true,
      fields := [.role "subject" ["Statement"] false (some "out") none,
        .role "object" ["Statement"] false (some "in") none] },
    { name := "MemberOf", kind := .relation, parent := some "Relation", abstract := false, pfx := 25,
      colocate := some "collection", onRetract := [("collection", .restrictOpen)],
      fields := [.role "member" ["Endurant"] false (some "memberships") (some "subject"),
        .role "collection" ["Object", "Event", "Organization"] false (some "members") (some "object"),
        .role "role" ["RoleKind"] true (some "member_roles") none] },
    { name := "ConnectsTo", kind := .relation, parent := some "Relation", abstract := false, pfx := 26,
      symmetric := true,
      fields := [.role "a" ["Statement"] false (some "connects") (some "subject"),
        .role "b" ["Statement"] false (some "connects") (some "object")] },
    { name := "DerivesFrom", kind := .relation, parent := some "Relation", abstract := false, pfx := 27,
      fields := [.role "lower" ["Purpose"] false (some "derives_from") (some "subject"),
        .role "higher" ["Purpose"] false (some "derived") (some "object")] },
    { name := "ClassifiedAs", kind := .relation, parent := some "Relation", abstract := false, pfx := 28,
      fields := [.role "instance" ["Node"] false (some "proposed_kinds") (some "subject"),
        .role "kind" ["ProposedKind"] false (some "proposed_instances") (some "object")] },
    { name := "Alternatives", kind := .relation, parent := some "Asserted", abstract := false, pfx := 29,
      multi := true,
      fields := [.role "source" ["Purpose"] false (some "interpretations") none,
        .setRole "members" ["DerivesFrom"] none] },
    { name := "AssertedBy", kind := .relation, parent := none, abstract := false, pfx := 30,
      fields := [.role "statement" ["Statement"] false (some "asserted_by") none,
        .role "asserter" ["Agent"] false (some "assertions") none] }]

/-- The subset's bases: `db:` for its vocabulary and instances, the graph `{base}ontology` that
`graph ontology : deep_brain @default` declares (`SCHEMA.md` section 6.11), and `Teloid`'s `action_identifier` under
its `@iri`. -/
def bases : Bases where
  vocab := "https://example.org/deep_brain#"
  inst := "https://example.org/deep_brain#"
  graph := "https://example.org/deep_brain#ontology"
  iris := [("action_identifier", "https://example.org/deep_brain#teloid_action_identifier")]

/-! ## Building values -/

/-- A record of the label `l`'s slots in their order, a slot not given absent. -/
def record (l : Name) (kvs : List (Name × VTerm typeside)) : VTerm typeside :=
  .record ((catalog.slots l).map fun s => (s.1, ((kvs.find? (·.1 == s.1)).map Prod.snd).getD absent))

/-- A reference through a role admitting `ps`, tagged when it admits several types. -/
def ref (ps : List Name) (l : Name) (e : Nat) : VTerm typeside :=
  match catalog.participantType ps with
  | .lbl _ => .elem e
  | _ => .inj l (.elem e)

/-- An instant at second `s` after the epoch, in UTC. -/
def instant (s : Int) : VTerm typeside := prim .dateTime (s * nanosPerSecond, 0)

/-- `asserted`: from second `s`, retracted at `r` when given. -/
def asserted (s : Int) (r : Option Int) : VTerm typeside :=
  .record [("end", match r with | some r => present (instant r) | none => absent), ("start", instant s)]

/-- The envelope every statement carries: asserted at `s`, not inferred, normal rank. -/
def envelope (s : Int) : List (Name × VTerm typeside) :=
  [("asserted", asserted s none), ("inferred", prim .bool false), ("rank", .inj "normal" (.record []))]

/-- An enumeration's member. -/
def member (m : Name) : VTerm typeside := .inj m (.record [])

/-- A measure. -/
def measureOf (x : F64) (u : String) : VTerm typeside := .record [("unit", prim .unit u), ("value", prim .float64 x)]

/-! ## Instance sets -/

/-- The `n`th instance set. -/
def instances (n : Nat) : FinApg catalog.schema where
  elems :=
    let k := n + 1
    let t0 : Int := 1704067200
    let retracted (i : Nat) : Option Int := if i = 1 then some (t0 + 3600 * (i : Int) + 60) else none
    [(1, "Company", record "Company" (envelope t0 ++ [("id", prim .string "AGENT:acme"),
        ("identifiers", present (.set [prim .string "lei:5299001", prim .string "ror:01abc"]))])),
     (2, "RoleKind", record "RoleKind" (envelope t0 ++ [("id", prim .string "KIND:engineer"),
        ("alt_labels", present (.set [prim .text ("Engineer", some "en"), prim .text ("Ingenieur", some "de")]))])),
     (3, "ProposedKind", record "ProposedKind" (envelope t0 ++ [("id", prim .string "KIND:router"),
        ("type_name", prim .string "Router"), ("supertype", member "Object")])),
     (4, "Quantity", record "Quantity" (envelope t0 ++ [("id", prim .string "QTY:length"),
        ("value", present (measureOf ⟨25, -1⟩ "m")), ("quantity_kind", prim .quantityKind "Length"),
        ("uncertainty", present (.record [("coverage_factor", present (prim .float64 ⟨1, 1⟩)),
          ("coverage_probability", absent), ("kind", member "expanded"), ("unit", prim .unit "m"),
          ("value", prim .float64 ⟨1, -3⟩)]))])),
     (5, "Event", record "Event" (envelope t0 ++ [("id", prim .string "EVENT:launch"),
        ("altitude", present (measureOf ⟨75, 4⟩ "[ft_i]")), ("where_uncertainty", present (prim .float64 ⟨5, 0⟩)),
        ("where", present (prim .geo "{\"type\":\"Point\",\"coordinates\":[8.5,47.4]}")),
        ("valid", present (.record [("end", absent), ("end_precision", absent),
          ("start", present (instant (t0 - 86400))), ("start_precision", present (prim .precision .day))]))])),
     (6, "Teloid", record "Teloid" (envelope t0 ++ [("id", prim .string "TELOID:root"),
        ("teloid_id", prim (.int .u64) 1), ("action_identifier", prim .string "halt"),
        ("modality", member "obligatory"), ("timestamp", prim (.int .i64) t0), ("specificity", prim (.int .i32) 1),
        ("priority", prim (.int .i32) 10), ("predicate_kind", member "deterministic"),
        ("predicate_id", prim .string "pred:root"), ("state", member "active"), ("version", prim (.int .u32) 1),
        ("meta", present (.list [.record [("key", prim .string "owner"), ("value", prim .string "ops \"core\"")],
          .record [("key", prim .string "tier"), ("value", prim .string "1")]])),
        ("tags", present (.set [prim .string "safety"]))]))] ++
    ((List.range k).flatMap fun i =>
      let t := t0 + 3600 * (i : Int)
      [(100 + i, "Person", record "Person" (envelope t ++ [("id", prim .string ("AGENT:p" ++ toString i)),
          ("personal_name", if i % 2 = 0 then present (prim .text ("Ada " ++ toString i, none)) else absent),
          ("personal_identifiers", present (.set [prim .string ("orcid:0000-" ++ pad 4 i)]))])),
        (200 + i, "Object", record "Object" (
          [("asserted", asserted t (retracted i)), ("inferred", prim .bool (i % 2 = 1)),
            ("rank", member (if i = 0 then "preferred" else "normal")),
            ("id", prim .string ("OBJ:o" ++ toString i)), ("level", present (prim (.int .u8) (i % 256))),
            ("confidence", present (prim .float64 ⟨3, -2⟩)), ("method", present (member "import"))])),
        (300 + i, "Teloid", record "Teloid" (envelope t ++ [("id", prim .string ("TELOID:t" ++ toString i)),
          ("teloid_id", prim (.int .u64) (100 + i)), ("action_identifier", prim .string ("inspect-" ++ toString i)),
          ("modality", member "permitted"), ("timestamp", prim (.int .i64) t),
          ("specificity", prim (.int .i32) (i + 2)), ("priority", prim (.int .i32) (-(i : Int))),
          ("predicate_kind", member (if i % 2 = 0 then "deterministic" else "uncertain")),
          ("predicate_id", prim .string ("pred:t" ++ toString i)), ("state", member "draft"),
          ("version", prim (.int .u32) 2),
          ("efficacious", present (.record [("end", present (instant (t0 + 31622400))),
            ("end_precision", present (prim .precision .month)), ("start", present (instant t)),
            ("start_precision", present (prim .precision .nanosecond))]))])),
        (400 + i, "MemberOf", record "MemberOf" (envelope t ++ [("member", ref ["Endurant"] "Person" (100 + i)),
          ("collection", ref ["Object", "Event", "Organization"] "Company" 1),
          ("role", if i % 2 = 0 then present (ref ["RoleKind"] "RoleKind" 2) else absent)])),
        (500 + i, "ConnectsTo", record "ConnectsTo" (("asserted", asserted t (retracted i)) :: envelope t ++
          [("a", ref ["Statement"] "Object" (200 + i)), ("b", ref ["Statement"] "Event" 5)])),
        (600 + i, "DerivesFrom", record "DerivesFrom" (envelope t ++ [("lower", ref ["Purpose"] "Teloid" (300 + i)),
          ("higher", ref ["Purpose"] "Teloid" 6)])),
        (700 + i, "AssertedBy", record "AssertedBy" [("statement", ref ["Statement"] "Object" (200 + i)),
          ("asserter", ref ["Agent"] "Person" (100 + i))]),
        (800 + i, "Alternatives_members", .record [("instance", .elem 900),
          ("participant", ref ["DerivesFrom"] "DerivesFrom" (600 + i))])]) ++
    [(900, "Alternatives", record "Alternatives" (envelope t0 ++ [("source", ref ["Purpose"] "Teloid" 6)])),
     (901, "ClassifiedAs", record "ClassifiedAs" (envelope t0 ++ [("instance", ref ["Node"] "Object" 200),
        ("kind", ref ["ProposedKind"] "ProposedKind" 3)]))]

end ApgModel.Oracle.Gen5
