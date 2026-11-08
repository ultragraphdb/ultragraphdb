# Preparing the APG proofs in Lean

* Status: plan, its decisions taken 2026-11-08 (section 4); wave L0 begins the work.
* Specifies: group A of `prepare-stage-6.md`, extended from its tasks A.1 to A.6 to every formal statement of
  `specification/schema/SCHEMA-APG.md`.

This note plans the Lean model of `SCHEMA-APG.md`: every definition transcribed, every proposition proved, and the
executable views and generator the test oracle of its section 19 runs. It fixes the model's representation, its module
layout, the proof of each statement, and the order in which they are written.

## 1. Done when

1. `lake build ApgModel` in `lean/` is green, with no `sorry`, no `admit` and no `axiom` declared.
2. `#print axioms` of every theorem lists nothing beyond Lean's own `propext`, `Quot.sound` and `Classical.choice`,
   and of every executable definition, nothing beyond `propext` and `Quot.sound`: the oracle computes.
3. Every definition, proposition and corollary of `SCHEMA-APG.md` sections 2 to 18 has its Lean counterpart, named in
   the index of section 6 and in a theorem table in `lean/schema/ApgModel/Theorems.lean`, as `PlacementModel`'s has.
4. `lake exe apg_oracle <dir>` writes into the folder it is given, for each schema of the corpus, generated valid APGs
   and their `R(G)` as N-Quads, with the Lean commit that wrote them; each export parses back to `R(G)` in Lean. Stage
   6's group 9 gives it the `testdata/apg/` folder of the test it writes.
5. Examples 6.7 and 9.6 of `SCHEMA-APG.md` evaluate in Lean to the values the specification states.
6. Each statement is compared with the paper's Coq development, `../../fork/APG`, where it has a counterpart
   (section 3), and the comparison is recorded in the module's documentation.

## 2. Where it starts

- **The Lean project.** `lean/` holds `RoundModel` and `PlacementModel`, built by `lake` outside Bazel, Lean 4.14
  pinned in `lean/lean-toolchain`, no package dependencies, `autoImplicit` off, files headed by the SPDX notice, and a
  `Theorems.lean` per library indexing its results with the simulator invariant each carries. The new library is
  `ApgModel`, a `lean_lib` of the same package whose sources sit under `lean/schema/` (`srcDir := "schema"`), so every
  Lean model of the schema has one folder while the package keeps one toolchain and one `lake build`; the
  `lean_exe apg_oracle` has its root there too.
- **No Mathlib.** `SCHEMA-APG.md` section 20 keeps the toolchain without Mathlib: no `Finset`, no `Fintype`, no
  category theory library. The model states its categorical claims concretely, as laws of identities and composition,
  and finiteness as enumeration by a list.
- **The specification.** Part 1 (sections 2 to 9) is the paper's core; part 2 (section 12) collections; part 3
  (sections 13 to 19) the catalog's schema, constraints, history, migration operations and the two views. Section 20
  decides a **binary core with a named layer**: the core over the grammar `0 | 1 | + | × | prim | lbl`, the named
  layer over records and variants with field names, proposition 3.3 between them.
- **MGQL.** `../mgql` (`github.com/marvin-hansen/mgql`, MIT) is a Lean 4.14 package with no dependencies. Its
  `MGQL/Schema.lean` defines `PropertyGraph`, `GraphSchemaFull` and `graphConformsSchema`, the checker proposition 18.2
  names, so the GQL view can be built as MGQL's own graph and checked by MGQL's own function.

## 3. The paper's Coq development

The paper's Coq proofs (paper footnote 13) are the repository `github.com/CategoricalData/APG`, held at
`../../fork/APG` beside this repository, commit `7190194`, read on 2026-11-08. Coq's standard library only. Three files:

| File | Lines | Holds |
|---|---|---|
| `APG.v` | 816 | The current development, condensed to the paper's results: category theory over `Type`, the types, the free bicartesian category of a schema, APGs, morphisms, schema mappings |
| `APG_old.v` | 1,486 | The earlier, long version, also published as `categoricaldata.net/APG.v`: a first generation of APGs with products, equalizers and coequalizers, beside the second generation `APG.v` keeps |
| `caml.v` | 796 | The monad comprehension calculus: a point-free calculus with products, sums, lists and streams, folds and distributivity, and its equational theory |

`APG.v`'s definitions, against this file's:

| | `APG.v` | `SCHEMA-APG.md` |
|---|---|---|
| Types | `Ty L`: `Zero`, `One`, `Plus`, `Times`, `Base b`, `Label l` | the binary core of definition 3.2 |
| Schema | `schema`: `L0 : Type`, `SIGMA : L0 → Ty L0` | definition 4.1 |
| APG | `apg S`: `E0 : L0 S → Type`, `PHI l e : TERM E0 (SIGMA S l)`, an intrinsically typed value | definition 6.1 |
| Morphism | `apgMorphism`: `LMAP` per label, with `PFNAT`, the values commuting | definition 7.1 |
| Mapping terms | point-free `term a b`: `IDEN`, `COMP`, `PAIR`, `CASE`, `FST`, `SND`, `INL`, `INR`, `tT`, `fF`, `ELEM l`, `PRIM` | definition 8.5, with a variable `x` and binders |
| Equality of terms | `termEq`: congruence, the category laws, terminal and initial, β and η for products and sums | none syntactic (decision L3) |
| `C_S` | `freeBCC`: `term` modulo `termEq`, quotiented through a declared `axiomQ` | definition 8.2, **distributive** |
| Schema mapping | `apgMapping`: `oMap : L0 S → Ty (L0 T)`, `aMap l : term (oMap l) (MapTy oMap (SIGMA S l))` | definition 9.1 |

What `APG.v` proves, every proof closed and none `Admitted`:
- the category of APGs on a schema, its initial APG and its coproducts (`apgInstInitial`, `apgHasCoProducts`);
- `freeBCC` a category; an APG's functor into `SET` (`apgToFunctor`), bicartesian (`apgIsBc`); a bicartesian functor's
  APG (`functorToAPG`); a mapping's functor between theories (`apgMtoF`), bicartesian (`apgMappingIsBc`), and back
  (`apgFtoM`).

What it does not hold:
- a theorem that the two constructions of proposition 8.4 form an equivalence;
- distributivity: `termEq` has no distributive law, so `freeBCC` is the free bicartesian category, not this file's
  distributive `C_S`, whose `case` reads `x` (section 8.2);
- `Δ` and its functoriality, proposition 9.3;
- finiteness, records with field names, collections, and anything of part 3.

`caml.v` holds what `APG.v` leaves out of the calculus: `Exp` with distribution (`UNDIST`), lists and their folds
(`FOLD0`, `PFOLD`), streams, `MAP_list`, `FLATTEN_list`, and an equational theory `ExpEq` with its soundness in sets,
`axiomsSound : ExpEq e1 e2 → ExpD e1 = ExpD e2`. It is the reference for a syntactic equality of distributive terms
with collections, should a later result need one stated.

The axioms differ between the files. `APG.v` uses functional extensionality, proof irrelevance, `JMeq`, classical
epsilon, and its own `axiomQ`, a quotient for every setoid. `caml.v` declares functional extensionality, set
extensionality, stream extensionality, and decidable equality for every type. Lean needs none of these as an axiom:
`Quot` is built in, functional extensionality follows from `Quot.sound`, proof irrelevance is definitional, and the
model's decidable equalities are proved per type.

The comparison has four consequences for this plan:
- the core takes `APG.v`'s shape, a family `E : L → Type` with intrinsically typed values (decision L1);
- proposition 8.4 goes beyond what `APG.v` proves, and over a distributive theory it does not build (decision L4);
- `Δ` (definition 9.2, propositions 9.3 to 9.5) and part 3 have no Coq counterpart to compare;
- `caml.v`'s `axiomsSound` is the model for proving a term equality sound, beside `eval`, if one is ever stated.

## 4. Decisions

Decided 2026-11-08, each as recommended.

**L1. How elements are represented.**
- *Decided:* two representations, joined by proposition 6.3.
  - The **core** holds an APG as a family `E : L → Type` with `υ : (l : L) → E l → V E (σ l)`, `V` computing a Lean type
    by recursion on the binary type, as the Coq second generation does. Functoriality, homomorphisms, the coalgebra
    view, terms, `Δ` and proposition 8.4 are proved here, where `V` is a type and a proof is structural induction.
  - The **executable layer** holds a finite APG in single-set form: elements numbered by `Nat`, each with a label and
    a value term (definition 5.4), checked against its label's named type by a decidable `check`. The views, the
    constraints' violation queries, the generator and the history model run here.
  - Proposition 6.3 is proved for both, and a lemma embeds a finite single-set APG into the core with its values, so
    the core's results apply to what the oracle generates.

**L2. Collections in the types from the start.**
- *Decided:* the core's `Ty` holds `list t` and `set t` from its first version, with the side conditions of
  definition 12.1 as a predicate. Corollary 12.3 then holds because propositions 5.3 to 8.4 are proved over the
  extended grammar, not by proving them twice.

**L3. Mapping terms and their equality.**
- *Decided:* the terms of definition 8.5 as an intrinsically typed inductive family over a context, a list of
  types holding `x` and the variables `case` and `map` bind, with de Bruijn indices. Evaluation is structural recursion
  on the term, total by construction, which is proposition 8.7. Two terms are equal when they evaluate equally on
  every APG. The syntactic theory of β, η and distributivity is not formalised, since `SCHEMA-APG.md` section 21 leaves
  the syntactic equality of terms undecided; the laws of definition 12.4 are proved as equalities of evaluation.

**L4. The scope of proposition 8.4.**
- *Decided:* the **generator form**. A model is an interpretation of the generators, a type `M l` per label and
  a function `δ_l : M l → V M (σ l)`, which is an APG; the theorem is that evaluation of terms is the unique extension
  of an interpretation that preserves products, coproducts and primitives, and that APGs and interpretations, with
  their homomorphisms and natural maps, are in bijection. This proves the content of 8.4 without building `C_S` as a
  category object.
- *Not taken now:* the **Coq route**, made axiom-free. `C_S` built as the quotient of point-free terms by an inductive
  equality holding `APG.v`'s bicartesian laws and `caml.v`'s distributive ones, through Lean's `Quot`; models as
  structure-preserving functors into Lean's types; the equivalence of categories proved in full. It is the faithful
  statement and the larger proof, and goes past what the Coq development proves; it is taken when a later result
  needs `C_S` as an object, as a proof about composing migrations as functors of theories would.

**L5. Literals, in proofs and in the export.**
- *Decided:* the RDF view's objects are structured values in Lean, a datatype with the value, never strings, so
  proposition 17.3 compares values. Serialisation to N-Quads lives in the exporter alone, beside a parser of the same
  format in Lean, and `#eval` checks that parsing every exported file gives back `R(G)`. The exporter's lexical forms
  follow `PROTOCOL.md` section 10.5, and the Rust side reads them as written.

**L6. The oracle's typeside.**
- *Decided:* the primitives Gen5 and the aviation example write in stage 6, each with decidable equality and a
  decidable linear order, which sets need:
  - `Bool`, `Int` with each integer width's range as a predicate, `Decimal` as an `Int` scaled by `10^s`;
  - `String`, and `Text` as a string with an optional language tag;
  - `Date`, `DateTime`, `LocalDateTime`, `Time` and `Duration` encoded as integers, with their canonical lexical forms
    for the export, and `Precision`;
  - `Iri`, `Unit` and `QuantityKind` by name, and `Geo` as its GeoJSON text;
  - `Float64` as the finite decimals exactly representable in binary64, a mantissa and an exponent, since Lean's `Float`
    has no decidable equality.
  `Vector` is left out until stage 8 stores vectors; the core's results do not depend on the typeside's choice.

**L7. Finiteness without Mathlib.**
- *Decided:* a type is finite when a list enumerates it, `∃ l : List α, ∀ a, a ∈ l`. Proposition 9.4 builds the
  enumeration of `V E t` from those of `E` and of the primitives.

**L8. Where part 3 gets its schemas.**
- *Decided:* as group A's task A.3 states: a Lean `Catalog` structure written by hand for the aviation example
  and a Gen5 subset holding every relation shape, translated to `S(K)` in Lean (section 13); the compiler's export of
  the same structure replaces the hand-written ones after stage 6's group 4 (task A.6).

**L9. The history model behind proposition 15.2.**
- *Decided:* a state is a finite APG with versions per element; a commit is a list of operations, insert,
  revise, retract and delete, applied with the actions of `SCHEMA-APG.md` section 14.3 (`@on_delete`, `@on_retract` with
  `restrict open`, `@retracts`, `@ends`) and refused where a rule refuses; the theorem holds of every reachable state,
  by induction over commits, as `PlacementModel`'s theorems hold of every reachable state.

**L10. SHACL, corollary 17.4.**
- *Decided:* one lemma per shape condition `SCHEMA.md` section 11.2 publishes, minimum and maximum count,
  datatype, `sh:in`, class, stated over the quads of `R(G)`. No SHACL engine is modelled.

**L11. MGQL as a dependency.**
- *Decided:* `lean/lakefile.lean` requires MGQL from `github.com/marvin-hansen/mgql` at a pinned commit, today
  `4ce4878`. `P(G)` is built as MGQL's `PropertyGraph`, `Γ(S(K))` as its `GraphSchemaFull`, and proposition 18.2 is
  `graphConformsSchema (P G) (Γ S) = true`. The same dependency serves stage 7's differential queries through MGQL's
  evaluator. `RoundModel` and `PlacementModel` do not import it.

## 5. Layout

`lean/schema/ApgModel.lean` imports every module; every path below is under `lean/schema/`. Each module's documentation
names the sections of `SCHEMA-APG.md` it transcribes, and each theorem's the proposition it proves.

| Module | Holds | `SCHEMA-APG.md` |
|---|---|---|
| `schema/ApgModel/Typeside.lean` | The typeside structure, definition 2.1 | 2 |
| `schema/ApgModel/Core/FinSet.lean` | Finite sets as lists up to their members, extensional | 12.1 |
| `schema/ApgModel/Core/Ty.lean` | Binary types with `list` and `set`, `lbls`, label-free, finite types | 3, 12.1 |
| `schema/ApgModel/Core/Value.lean` | `V`, its action on families, functoriality, label-free independence | 5, 12.2 |
| `schema/ApgModel/Core/Apg.lean` | APGs in both forms, the element graph, referential integrity | 6 |
| `schema/ApgModel/Core/Hom.lean` | Homomorphisms, isomorphisms, the category laws, the coalgebra view | 7, 8.1 |
| `schema/ApgModel/Core/Term.lean` | Mapping terms, evaluation, `map` and `filter` | 8.3, 12.4 |
| `schema/ApgModel/Core/Model.lean` | Proposition 8.4 in the generator form (L4) | 8.2 |
| `schema/ApgModel/Core/Delta.lean` | Schema mappings, `Φ̂`, `ι`, `Δ`, its functoriality, propositions 9.4 and 9.5 | 9 |
| `schema/ApgModel/Core/DeltaCompose.lean` | The identity and composite mappings, terms along a mapping, `Δ` along both | 9.3 |
| `schema/ApgModel/Named/Ty.lean` | Field names, named types with strictly increasing field names, decidable equality by hand, the element kinds | 3.1, 3.2, 4.2 |
| `schema/ApgModel/Named/Value.lean` | Named values, records as functions from their fields and variants as a field with a value; named APGs | 5.1, 5.2, 6.1 |
| `schema/ApgModel/Named/Translate.lean` | Proposition 3.3 for values, natural in the family; named APGs and their binary forms | 3.3 |
| `schema/ApgModel/Named/Term.lean` | Named mapping terms: records, fields, injections and cases by field; their evaluation | 8.5, 8.6 |
| `schema/ApgModel/Named/TermTranslate.lean` | Proposition 3.3 for terms: the translation and evaluation commuting with it | 3.3 |
| `schema/ApgModel/Named/Apg.lean` | Value terms, `check`, finite single-set APGs, the embedding into the core | 5.4, 5.5, 6.2 |
| `schema/ApgModel/Named/Collections.lean` | Role labels, symmetric relations, unnest | 12.3 to 12.5 |
| `schema/ApgModel/Examples.lean` | Example 6.7, checked by the kernel; example 9.6 with wave L3; example 6.7's catalog, its `S(K)` and constraints with wave L6 | 6.7, 9.6, 13, 14 |
| `schema/ApgModel/Catalog/Schema.lean` | `Catalog`, `conc`, `S(K)`, prefix ranges, proposition 13.1 | 13 |
| `schema/ApgModel/Catalog/Constraints.lean` | Violation queries and their soundness, transition constraints | 14 |
| `schema/ApgModel/History.lean` | Naming, graphs, states, slices, commits, proposition 15.2 | 15 |
| `schema/ApgModel/Migration.lean` | Selection, key lookup, extended terms, the fragment, coproducts, proposition 16.6 | 16 |
| `schema/ApgModel/Views/Rdf.lean` | `R` from templates natural in their references, proposition 17.2 (L7a) | 17 |
| `schema/ApgModel/Views/RdfInverse.lean` | `R⁻¹` (L7b) | 17 |
| `schema/ApgModel/Views/RdfRoundTrip.lean` | Proposition 17.3 (L7b) | 17 |
| `schema/ApgModel/Views/RdfShacl.lean` | Corollary 17.4 (L7b) | 17 |
| `schema/ApgModel/Views/Gql.lean` | `P` as MGQL's graph, `Γ`, propositions 18.2 to 18.4 | 18 |
| `schema/ApgModel/Oracle/Typeside.lean` | The oracle's concrete typeside (decision L6), with wave L7a | 2 |
| `schema/ApgModel/Oracle/Generate.lean`, `Oracle/NQuads.lean`, `Oracle/Main.lean` | The generator of valid APGs, the N-Quads exporter and parser, `apg_oracle` | 19 |
| `schema/ApgModel/Theorems.lean` | The index of every result, with the Rust check each feeds | all |

## 6. Every statement and its proof

Sizes are the expected effort: S a short induction or a definition's unfolding, M a page of lemmas, L the module's
main work.

| Statement | Lean | Proof | Size |
|---|---|---|---|
| Definitions 2.1, 3.2, 4.1 | `Typeside`, `Core.Ty`, `Core.Schema`, with definition 12.1's admitted collections as `Ty.Admitted` | Definitions | S |
| Definitions 3.1, 4.2 | `Named.Name`, the element kinds as predicates on named types | Definitions | S |
| Proposition 3.3, values | `toBin`, `ofBin`, `ofBin_toBin`, `toBin_ofBin`, `toBin_map`; `NApg.toCore`, `NApg.ofCore`, `NApg.comm_iff` | Recursion through the field lists, each record or variant field by field: the associativity and unit isomorphisms of `×` and `+` | L |
| Proposition 3.3, terms | `NTm.bin`, `NTm.eval_bin`, with `recAt`, `projAt`, `injAt`, `caseAt` and their evaluation lemmas | Induction on the term; a `case` as nested binary cases, later branches weakened past the variable each outer case binds | L |
| Definitions 5.1, 5.2 | `V`, `V.map` | Recursion on `Ty` | S |
| Proposition 5.3 | `V.map_id`, `V.map_comp` | Induction on `t`, functional extensionality | S |
| Definitions 5.4, 5.5 | `VTerm`, `denote`, `check`, `check_iff`, `denote_complete`, `VTerm.refs`, `denote_refs` | The check is a successful denotation; completeness by recursion through the field lists, for well-formed types | M |
| Definitions 6.1, 6.2; proposition 6.3 | `Apg`, `SingleSet`, `toSingleSet`, `ofSingleSet`, the two round trips | Sigma types over labels | M |
| Definitions 5.5, 6.5; proposition 6.6 | `V.Ref`, `Apg.Arc`, `SingleSet.Arc`, `SingleSet.arc_target_label` | `V E (lbl l) = E l`; a reference carries the label its position's type names, the path of field names coming with the named layer | S |
| Definitions 7.1, 7.3; proposition 7.2 | `Hom`, `Hom.id`, `Hom.comp`, `id_comp`, `comp_id`, `comp_assoc`, `Iso` | Componentwise, from proposition 5.3 | S |
| Proposition 8.1 | `coalgebra_equiv`, `hom_iff_coalgebra_morphism` | Unfolding | S |
| Definitions 8.2, 8.3; proposition 8.4 | `Model`, `Model.ofApg`, `Model.toApg`, `Model.interp_eq_eval`, `Model.ofApg_toApg`, `Tm.eval_hom`, `Model.Hom.natural` | Generator form: uniqueness by induction on terms; homomorphisms commute with evaluation | M |
| Definitions 8.5, 8.6; proposition 8.7 | `Tm S Γ t`, `Tm.eval`, `Tm.eval_terminates`, the cyclic example | Structural recursion; the theorem records that `eval` is a total function with no `partial`, and a two-element cycle evaluates by `rfl` | M |
| Definitions 9.1, 9.2 | `Mapping`, `Ty.subst`, `iota`, `iotaInv`, `Mapping.delta`, `Mapping.deltaHom` | `ι` by recursion on `t`, the identity at labels; `iota_map` its naturality | M |
| Proposition 9.3 | `deltaHom_id`, `deltaHom_comp`; `Mapping.id`, `delta_id`; `Tm.along`, `Tm.eval_along`; `Mapping.comp`, `delta_comp` | Functor laws by proposition 5.3; composition by translating terms along a mapping, with `iota_comp` across `Ty.subst_subst` by heterogeneous equality | L |
| Proposition 9.4 | `V_enumerable`, `Mapping.delta_finite` | Enumeration of `V E t` for list-free `t` with finite primitives (L7), a set by its sublists | M |
| Proposition 9.5 | `labelSum`, `labelSumBij`, `delta_label_image`, `delta_one_origin` | A variant of labels is in bijection with the labels' elements, each tagged by its position | S |
| Definition 12.1; proposition 12.2 | `LabelFree`, `V_labelFree_const`, `V.map_labelFree_id` | Induction on `t` | S |
| Corollary 12.3 | `corollary_12_3`, citing the results it carries | Holds by L2 | S |
| Definition 12.4 | `Tm.map`, `Tm.filter`, `Tm.mapSet`, `Tm.filterSet`, `Tm.DerefFree`; `eval_map_id`, `eval_map_map` and their set forms | Through `Tm.subst`, with `eval_rename` and `eval_subst`; sets through `FinSet.map`'s laws | M |
| Definitions 12.5, 12.6, 12.7 | `roleLabelType`, `symNav`, `symNav_symm`, `unnest`, `unnest_origin` | Case analysis on a binary element's two participants; recursion over the elements | M |
| Section 13; proposition 13.1 | `Catalog`, `conc`, `schema`, `ranges`, `inRanges_compress`, `inRanges_iff_isSub`, `is_iff_inRanges` | A tree of types; prefixes injective; ranges as the compression of the sorted prefix set | M |
| Section 14.2 | `keyViolations`, `uniqueViolations`, `nonEmptyViolations`, `setViolations`, `roleLabelViolations`, `participationViolations`, `checkViolations`, `acyclicViolations` with `maxDepthViolations`, `noOverlapViolations`, `limitViolations` with `limitGroupViolations`; a `_nil_iff` per row | Three generic queries, `injViolations`, `violators` and `cycleViolations`, each proved once; cycles by the pigeonhole over walks; `@limit`'s group selection as the conjuncts over participant types | M |
| Section 14.3 | `State` and its predicates `LabelFixed`, `FieldsFixed`, `AppendOnly`, `Bitemporal`, `OnDeleteHolds`, `OnRetractHolds`, `RetractsHolds`, `EndsHolds`, `LiveParticipants` | Definitions, used by section 15 | S |
| Section 15.1 | `Naming`, `naming_injective`, `edge_named_by_triple` | The edge case from `@set` exclusivity | M |
| Proposition 15.1 | `union_closed` | Disjoint element sets; closure by construction | S |
| Section 15.3; proposition 15.2 | `State`, `Commit`, `step`, `liveSlice`, `believedSlice`, `live_closed`, `believed_closed` | Induction over reachable states, one lemma per operation (L9) | L |
| Definition 16.1 | `select`, `select_apg`, `select_inclusion_injective`, `select_inclusion_preserves_names` | Definitional once closure holds | S |
| Definition 16.2 | `lookup`, `lookup_unique` | From key exclusivity | S |
| Definition 16.3 | `ExtTm`, `evalExt` | Mapping terms with lookups, inverse navigations, derivations and functions over a valid source | M |
| Proposition 16.4 | `fragment_denotes`, `fragment_one_origin` | Definitions 16.1, 16.3, unnest, proposition 9.5 | M |
| Definition 16.5 | `coprod`, `coprod_inl_hom`, `coprod_inr_hom` | Definitions | S |
| Proposition 16.6 | `history_through_migration` | Propositions 15.2 and 16.4 and the permanence of key claims | M |
| Definition 17.1; proposition 17.2 | `templates`, `Tmpl.inst`, `rdf`, `templates_mapRefs`, `Tmpl.inst_mono`, `rdf_monotone` | Each quad from one element's template, natural in its references, named by a naming monotone in its definedness | M |
| Proposition 17.3 | `rdfInv`, `RoundTrip`, `rdf_round_trip` | Attribution of each own predicate to its subject; label from the chain of type quads; each slot by inverting the check against its type; elements by IRI, triple or pair | L |
| Corollary 17.4 | `shacl_min_count`, `shacl_max_count`, `shacl_datatype`, `shacl_in`, `shacl_class`, `shacl_class_edge` | From the shapes of an element's own quads that proposition 17.3's proof establishes (L10) | M |
| Definition 18.1 | `gql : PropertyGraph`, `graphType : GraphSchemaFull` | Built as MGQL's types (L11) | M |
| Proposition 18.2 | `gql_conforms : graphConformsSchema (gql G) (graphType S) = true` | Every node's and edge's label set and properties by construction; endpoints by field typing | L |
| Proposition 18.3 | `gql_monotone` | As 17.2 | S |
| Proposition 18.4 | `gql_of_rdf` | From proposition 17.3 | S |
| Section 19 | `generate`, `exportNQuads`, `parseNQuads`, `parse_export` checked by `#eval` | Executable; not proofs | M |

## 7. Order

Each wave's modules depend only on earlier waves; a wave ends with `lake build` green and the user's commit.

| Wave | Modules | Needs | Ends with |
|---|---|---|---|
| L0 | Spike and setup | | `lakefile.lean` holding `ApgModel` with `srcDir := "schema"` and MGQL (L11) in `lake-manifest.json`; the spike's findings below |
| L1 | `Typeside`, `Core/Ty`, `Core/FinSet`, `Core/Value` | L0 | propositions 5.3 and 12.2 |
| L2 | `Core/Apg`, `Core/Hom`, `Core/Term` | L1 | propositions 6.3, 6.6, 7.2, 8.1, 8.7; definition 12.4's laws |
| L3 | `Core/Delta`, `Core/Model`, example 9.6 | L2 | propositions 8.4, 9.3, 9.4, 9.5; example 9.6 |
| L4 | `Named/Ty`, `Named/Translate` | L1, beside L2 and L3 | proposition 3.3 |
| L5 | `Named/Apg`, `Named/Collections`, `Examples` | L2, L4 | the executable layer; example 6.7 |
| L6 | `Catalog/Schema`, `Catalog/Constraints` | L5 | proposition 13.1; section 14 |
| L7a | The catalog's view data, `Oracle/Typeside`, `Views/Rdf` | L6 | each relation's encoding (`SCHEMA.md` section 11.1), statement types, interval, measure and catalog-type attributes, `asserted`; the oracle's typeside (decision L6); definition 17.1 and proposition 17.2 |
| L7b | `Views/Rdf` | L7a | `R⁻¹` and proposition 17.3; corollary 17.4 (decision L10) |
| L7c | `Oracle/Generate`, `Oracle/NQuads`, `Oracle/Main`, the corpus catalogs | L7b | the generator; the N-Quads exporter and parser, parse after export checked by `#eval`; the aviation and Gen5-subset catalogs (decision L8); `apg_oracle` exporting the corpus for stage 6's group 9 |
| L8 | `History`, `Migration` | L6 | propositions 15.1, 15.2, 16.4, 16.6 |
| L9 | `Views/Gql` | L7b | propositions 18.2 to 18.4, for stage 7 |
| L10 | `Theorems`, the axiom audit, the Coq comparison recorded | all | section 1's criteria |

**What L0 settled**, under Lean 4.14 without Batteries or Mathlib:
- structural recursion runs through a type nested in `List (Name × NTy)`, a type-valued function such as `V` included,
  so the named type stays nested; its constructors are `record` and `variant`, since `rec` is the recursor's name;
- `deriving DecidableEq` does not apply to the nested type, whose decidable equality is written by hand in L4;
- core Lean provides `List.Nodup`, `List.Pairwise` and the `List.map` lemmas the model uses;
- a finite set as a quotient of lists by their members (`Core.FinSet`) carries `map` and its laws with no axiom beyond
  `propext` and `Quot.sound`.

The `lean_exe apg_oracle` enters `lakefile.lean` with its root, `Oracle/Main.lean`, in wave L7c; it takes the output
folder as its one argument, and L7c runs it into a scratch folder whose files Lean parses back.

**What L2 settled:**
- a typeside function takes its arguments by position, `(i : Fin (dom f).length) → PV ((dom f).get i)`, so a term
  applying it holds one argument term per position and the terms need no second, mutual inductive type;
- the terms hold corollary 12.3's operations built from collections alone, `nil`, `cons`, `append`, `empty`, `insert`
  and `union`; `length`, `at`, `size` and `member` need counts and decidable equality of members, which the oracle's
  concrete typeside supplies;
- renaming and substitution of terms (`Tm.rename`, `Tm.subst`), with evaluation commuting with both, carry
  definition 12.4's laws and are what `Δ`'s composition in L3 substitutes with;
- proposition 6.3's second direction is a bijection of elements keeping labels and values (`toSingleSet_ofSingleSet_*`),
  the first an isomorphism of APGs (`ofSingleSet_toSingleSet`).

**What L4 settled:**
- a named record's value is a function from its fields and a variant's a field with a value, definition 5.1's
  product and sum over the fields; a field is its position, which a well-formed type's increasing names make the
  same as its name (`NTy.fieldIndex`);
- `NTy.V.VAt.out` and `into` pass between a field's value and a value of the field's type by the same recursion as
  the type, so no cast enters a definition or a statement;
- proposition 3.3 holds for every field list, so well-formedness (`NTy.WF`) is not among its premises; it is what makes
  one finite map of fields one value term in L5;
- `absurd s` is the `case` of the empty variant, and the named Booleans translate exactly to the core's `Ty.bool`.

**What L5 settled:**
- a value term checks when it denotes a value (`check_iff`), so the check and the denotation the core reads are one
  function; every value of a well-formed type has a term (`denote_complete`), well-formedness giving distinct field
  names, so a field named in a term is the field at its position (`NTy.fieldIndex_nameAt`);
- a record term lists its fields in the type's order, one term per value; a set term's duplicate members collapse, as
  `SCHEMA.md` section 5.4 accepts a write holding a duplicate;
- a finite APG numbers its elements by `Nat` and is valid when the numbers are distinct and every value checks;
  `FinApg.toNApg` makes a valid one an APG on the named schema, and so on the core's through proposition 3.3;
- example 9.6 is a migration along `Δ` and joins wave L3, which defines `Δ`.

**What L3 settled:**
- proposition 8.4 is proved in generator form: a model interprets every term with one equation per constructor, and
  its interpretation is evaluation (`Model.interp_eq_eval`), so a model through its APG and back is itself; evaluation
  commuting with homomorphisms (`Tm.eval_hom`) is what makes `Δ` a functor as well;
- the composite mapping's term has the type `Ψ̂(Φ̂(σ(l)))`, which equals `(Ψ∘Φ)^(σ(l))` by `Ty.subst_subst`, a proved
  equation: the composite casts its term once (`Tm.castTy`), and `iota_comp` and `iota_lbl` compare values across it
  by heterogeneous equality, so no cast appears in a statement;
- the identity and composite mappings give isomorphisms of APGs (`delta_id`, `delta_comp`) built with `Iso.ofHom`,
  a homomorphism whose components have inverse functions;
- proposition 9.4 is proved for images without a `list`, a set of a finite type enumerated by its sublists
  (`FinSet.enumerable`); a `list` in an image makes `Δ` infinite whatever the primitive types, so `SCHEMA-APG.md`
  section 9's finite-images rule excludes it (decided 2026-11-08);
- example 9.6 is checked by the kernel, the identity term of `AssertedBy` type-checking against `Φ̂(σ'(AssertedBy))`
  as the specification states.

**What L7b settled:**
- the view's predicates and class IRIs are structured values (`Pred`, `Term.cls`), as its literals are (decision L5);
  the exporter writes them as IRIs, distinct by `UGS0801`, so proposition 17.3 is proved over structured quads;
- the built-in fields are `ugdb:asserted_start`, `ugdb:asserted_end` and `ugdb:live` (`SCHEMA.md` section 7.4);
- an interval's bounds are both optional, at least one present, each precision present exactly when its bound is and
  shown where coarser than its type's own, so the inverse restores a missing one as the type's own (`SCHEMA.md`
  sections 5.1 and 5.5, `SCHEMA-APG.md` section 13.2);
- an edge-encoded relation has two required single-valued roles, and allocates a reifier when its tree holds more than
  one concrete type, whose `rdf:type` tells them apart (`SCHEMA.md` section 11.1): without either, `R` loses a
  participant or a type;
- a concrete type's record is its `slots`, which `S(K)` and the inverse both read;
- `rdfInv` reads an element named by an IRI from its type and field quads, one named by a triple from its navigation
  quad, and one of a role label from its role quad, references through the naming's inverse; on example 6.7 it gives
  back every element with its label and value, checked by the kernel;
- proposition 17.3 is `rdf_round_trip`, under conditions (`RoundTrip`) that each restate a rule of `SCHEMA.md`: field
  names unique, an attribute's view fitting its type, edge-encoded relations' anchors and navigation names (`UGS0404`,
  `UGS0408`, `UGS0418`), a valid `G` whose labels are `S(K)`'s, an injective naming defined on IRI-named elements and
  participants, and `G` written as the store writes it (sorted sets without an empty one, bounded intervals with each
  precision beside its bound, hidden precisions their type's own, `@symmetric` triples in IRI order); the string order's
  antisymmetry, which the last needs, is proved from the order of characters;
- corollary 17.4 is a lemma per shape condition, over the quads at an element's IRI under the same conditions:
  `sh:minCount 1` of a required attribute, `sh:maxCount 1` of a single-valued one, `sh:datatype` and `sh:in` of a
  primitive or enumerated attribute and of a set's members, and `sh:class` of a role triple's participant and of every
  navigation triple's object.

**What L7a settled:**
- the catalog carries what the views read: keys, `@bitemporal`, `@multi`, `@symmetric`, `@colocate`, navigation
  names and each attribute's view; `S(K)` gives a `@bitemporal` type its `asserted` field, and a relation its encoding,
  anchor and reifier allocation as `SCHEMA.md` section 11.1 states (`edgeEncoded`, `anchorRole`, `allocatesReifier`);
- an edge-encoded relation's anchor role declares a navigation name, its triple's predicate (`UGS0418`, decided
  2026-11-08);
- `R(G)` is every element's templates named: a template's subject and objects may be elements, named by `ι` when the
  view is taken, so an element without a name, an edge without a reifier or a role label's, contributes only the
  quads naming its participants; proposition 17.2 needs no injectivity, and holds for every homomorphism preserving
  names and graphs;
- the oracle's typeside holds the primitives Gen5 and the aviation example write, unsigned widths among them, each
  with decidable equality, a range check (`Prim.admits`) and a linear order (`Prim.cmp`);
- example 6.7 on the catalog's schema is valid and its `R(G)` checked by the kernel, quad by quad.

**What L6 settled:**
- a catalog lists its types with kind, parent, abstractness, prefix and own fields, a field's type already a named
  type; `S(K)` gives a concrete type the record of its fields, own and inherited, sorted by name, a specialised role
  replacing its super role, and a role label `R_r` per set-valued role, as example 6.7's catalog shows by `decide`;
- proposition 13.1 is the prefix-range check: the ranges are the compression of the sorted prefix set and hold
  exactly its members (`inRanges_compress`), so a type test by ranges is membership in `conc(T)`, given unique
  prefixes;
- section 14.2's ten rows rest on three generic queries, each proved once: injectivity where defined (`@key`,
  `@unique`, `@set`, role labels, `@no_overlap` through `pairViolations`), a test per candidate (non-empty set-valued
  roles, participation, `@check`, `@limit`), and walks (`@acyclic`, `max_depth`); a walk of as many references as
  there are vertices repeats one, so the bounded search finds every cycle (`pigeonhole`, `cycle_of_not_nodup`);
- value terms compare by decidable equality, given the primitives'; a set compares as written, so the oracle writes a
  set's members in one order without repetitions, and `@set` compares a set-valued role's participants as sets;
- the aviation and Gen5-subset catalogs of decision L8 need the oracle's typeside, decision L6, and come with wave L7c;
- liveness follows `SCHEMA.md` section 8.4, rule 5: `@set`, participation, `@limit`, `@no_overlap` and `@acyclic`
  read the live elements, `@key`, `@unique`, `@check` and non-empty set-valued roles every element a state holds; a
  participation or `@limit` lower bound asks only live participants (decided 2026-11-08);
- `max_depth n` admits chains of at most `n` references (decided 2026-11-08);
- `@limit` takes two forms: a lower bound names one `per` role and no `type` (`UGS0529`, decided 2026-11-08) and
  selects every live participant, one no element reaches included (`limitViolations`); an upper bound groups the
  elements by their participants and, under `type`, their concrete type (`limitGroupViolations`).

Stage 6 needs the oracle first, so the path L0, L1, L2, L4, L5, L6, L7 comes before L3, L8 and L9, which no stage 6
group waits on. In `prepare-stage-6.md`'s terms: tasks A.1 and A.2 are waves L1 to L4, A.3 is L5 to L7, A.4 the
properties of L7 that group 9's test checks, A.5 the findings each wave reports, and A.6 replaces L7c's hand-written
catalogs once stage 6's group 4 exports them.

## 8. Risks

- **Equalities of types.** `V E (subst τ t)` and `V (delta E) t` are equal types; a proof using the equality casts
  values. `iotaEquiv` is defined by recursion as an equivalence instead, so no cast enters a statement.
- **Proposition 15.2's model.** The history model restates part of `SCHEMA.md` section 8.4 operationally; a mismatch
  with the specification is a finding for it, reported under A.5.
- **Size.** `PlacementModel` holds about 2,700 lines, and `APG.v` 816 for part of part 1 without `Δ`; this model is
  expected between 6,000 and 9,000.
- **MGQL as a dependency.** A change of MGQL's `PropertyGraph` breaks `Views/Gql`; the pin holds it, and an update is
  a deliberate change.
