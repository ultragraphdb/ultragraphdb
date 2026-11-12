/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Examples
import ApgModel.Core.DeltaCompose
import ApgModel.Named.TermTranslate
import ApgModel.Named.Collections
import ApgModel.Views.RdfShacl
import ApgModel.Views.Gql
import ApgModel.History
import ApgModel.Migration
import ApgModel.Oracle.Check

/-!
# The theorems of `SCHEMA-APG.md`

Every definition, proposition, corollary and example of `specification/schema/SCHEMA-APG.md` sections 2 to 19 and its
Lean counterpart (`openspec/notes/archive/prepare-apg-lean.md`, sections 1 and 6). The compiler checks each name below
at the end of this file. Every theorem rests on Lean's `propext`, `Quot.sound` and `Classical.choice` alone, and every
executable definition, the oracle's included, on `propext` and `Quot.sound` at most; no `sorry`, `admit` or `axiom` is
declared.

| Statement | Lean |
|---|---|
| Definitions 2.1, 3.2, 4.1; definition 12.1's admitted types | `Typeside`, `Core.Ty`, `Core.Schema`, `Core.Ty.Admitted` |
| Definitions 3.1, 3.2 named, 4.2 | `Named.Name`, `Named.NTy`, `Named.NTy.WF`, `Named.NSchema.IsVertex`, `Named.NSchema.IsEdge`, `Named.NSchema.IsVertexProperty`, `Named.NSchema.IsEdgeProperty`, `Named.NSchema.IsMetaEdge`, `Named.NSchema.IsHyperedge` |
| Proposition 3.3, values | `Named.toBin`, `Named.ofBin`, `Named.ofBin_toBin`, `Named.toBin_ofBin`, `Named.toBin_map`, `Named.NApg.toCore`, `Named.NApg.ofCore`, `Named.NApg.ofCore_toCore`, `Named.NApg.toCore_ofCore`, `Named.NApg.comm_iff` |
| Proposition 3.3, terms | `Named.NTm.bin`, `Named.NTm.eval_bin`, `Named.recAt`, `Named.projAt`, `Named.injAt`, `Named.caseAt`, `Named.recAt_eval`, `Named.projAt_eval`, `Named.injAt_eval`, `Named.caseAt_eval` |
| Definitions 5.1, 5.2; proposition 5.3 | `Core.V`, `Core.V.map`, `Core.V.map_id`, `Core.V.map_comp` |
| Definitions 5.4, 5.5 | `Named.VTerm`, `Named.denote`, `Named.check`, `Named.check_iff`, `Named.denote_complete`, `Named.VTerm.refs`, `Named.denote_refs` |
| Definitions 6.1, 6.2, 6.4; proposition 6.3 | `Core.Apg`, `Core.SingleSet`, `Core.toSingleSet`, `Core.ofSingleSet`, `Core.ofSingleSet_toSingleSet`, `Core.toSingleSet_ofSingleSet_elements`, `Core.toSingleSet_ofSingleSet_lam`, `Core.toSingleSet_ofSingleSet_value`, `Named.FinApg`, `Named.FinApg.valid`, `Named.FinApg.toNApg`, `Core.Enumerable` |
| Definition 6.5; proposition 6.6 | `Core.V.Ref`, `Core.Apg.Arc`, `Core.SingleSet.Arc`, `Core.SingleSet.arc_target_label`, `Core.Apg.arc_target` |
| Example 6.7 | `Examples.apg`, `Examples.apg_valid`, `Examples.a1_references_m1`, `Examples.misfiled_invalid`, `Examples.duplicated_invalid` |
| Definitions 7.1, 7.3; proposition 7.2 | `Core.Hom`, `Core.Hom.id`, `Core.Hom.comp`, `Core.Hom.id_comp`, `Core.Hom.comp_id`, `Core.Hom.comp_assoc`, `Core.Iso` |
| Proposition 8.1 | `Core.Coalg`, `Core.Coalg.Hom`, `Core.Apg.toCoalg`, `Core.Coalg.toApg`, `Core.Coalg.toApg_toCoalg`, `Core.Apg.toCoalg_toApg`, `Core.Hom.toCoalg`, `Core.Coalg.Hom.toApg`, `Core.Hom.toCoalg_toApg`, `Core.Coalg.Hom.toApg_toCoalg` |
| Definitions 8.2, 8.3; proposition 8.4 | `Core.Model`, `Core.Model.ofApg`, `Core.Model.toApg`, `Core.Model.interp_eq_eval`, `Core.Model.toApg_ofApg`, `Core.Model.ofApg_toApg`, `Core.Tm.eval_hom`, `Core.Model.Hom.natural` |
| Definitions 8.5, 8.6; proposition 8.7 | `Core.Tm`, `Core.Tm.eval`, `Core.Tm.eval_terminates`, `Core.cyclicApg` |
| Definitions 9.1, 9.2 | `Core.Mapping`, `Core.Ty.subst`, `Core.iota`, `Core.iotaInv`, `Core.Mapping.delta`, `Core.Mapping.deltaHom` |
| Proposition 9.3 | `Core.Mapping.deltaHom_id`, `Core.Mapping.deltaHom_comp`, `Core.Mapping.id`, `Core.Mapping.delta_id`, `Core.Tm.along`, `Core.Tm.eval_along`, `Core.Mapping.comp`, `Core.Mapping.delta_comp` |
| Proposition 9.4 | `Core.V_enumerable`, `Core.Mapping.delta_finite` |
| Proposition 9.5 | `Core.labelSum`, `Core.labelSumBij`, `Core.Mapping.delta_label_image`, `Core.Mapping.delta_one_origin` |
| Example 9.6 | `Examples.migration`, `Examples.migrated`, `Examples.migrated_memberships`, `Examples.m1_collection`, `Examples.m1_member`, `Examples.m1_role`, `Examples.a1_names_m1` |
| Definition 12.1; proposition 12.2 | `Core.Ty.LabelFree`, `Core.V_labelFree`, `Core.V_labelFree_empty`, `Core.V.map_labelFree`, `Core.V.map_labelFree_self` |
| Corollary 12.3 | `corollary_12_3` |
| Definition 12.4 | `Core.Tm.map`, `Core.Tm.filter`, `Core.Tm.mapSet`, `Core.Tm.filterSet`, `Core.Tm.DerefFree`, `Core.Tm.eval_map_id`, `Core.Tm.eval_map_map`, `Core.Tm.eval_mapSet_id`, `Core.Tm.eval_mapSet_mapSet` |
| Definitions 12.5, 12.6, 12.7 | `Named.roleLabelType`, `Named.symNav`, `Named.symNav_symm`, `Named.unnest`, `Named.unnest_origin` |
| Section 13; proposition 13.1 | `Catalog.Catalog`, `Catalog.Catalog.conc`, `Catalog.Catalog.schema`, `Catalog.Catalog.ranges`, `Catalog.inRanges_compress`, `Catalog.Catalog.inRanges_iff_isSub`, `Catalog.Catalog.is_iff_inRanges` |
| Section 14.2 | `Catalog.injViolations`, `Catalog.violators`, `Catalog.cycleViolations`, `Catalog.keyViolations`, `Catalog.keyViolations_nil_iff`, `Catalog.uniqueViolations`, `Catalog.uniqueViolations_nil_iff`, `Catalog.nonEmptyViolations`, `Catalog.nonEmptyViolations_nil_iff`, `Catalog.setViolations`, `Catalog.setViolations_nil_iff`, `Catalog.roleLabelViolations`, `Catalog.roleLabelViolations_nil_iff`, `Catalog.participationViolations`, `Catalog.participationViolations_nil_iff`, `Catalog.checkViolations`, `Catalog.checkViolations_nil_iff`, `Catalog.acyclicViolations`, `Catalog.acyclicViolations_nil_iff`, `Catalog.maxDepthViolations`, `Catalog.maxDepthViolations_nil_iff`, `Catalog.noOverlapViolations`, `Catalog.noOverlapViolations_nil_iff`, `Catalog.limitViolations`, `Catalog.limitViolations_nil_iff`, `Catalog.limitGroupViolations`, `Catalog.limitGroupViolations_nil_iff` |
| Section 14.3 | `Catalog.State`, `Catalog.State.LabelFixed`, `Catalog.State.FieldsFixed`, `Catalog.State.AppendOnly`, `Catalog.State.Bitemporal`, `Catalog.State.OnDeleteHolds`, `Catalog.State.OnRetractHolds`, `Catalog.State.RetractsHolds`, `Catalog.State.EndsHolds`, `Catalog.State.LiveParticipants` |
| Section 15.1 | `Views.ident`, `History.Exclusive`, `History.naming_injective` |
| Proposition 15.1 | `History.union`, `History.union_closed` |
| Section 15.3; proposition 15.2 | `History.HState`, `History.Kinds`, `History.Live`, `History.Bel`, `History.LiveRead`, `History.BelRead`, `History.Inv`, `History.Step`, `History.Reachable`, `History.inv_step`, `History.inv_reachable`, `History.live_closed`, `History.believed_closed`, `History.liveRead_closed`, `History.belRead_closed`, `History.prop_15_2` |
| Definition 16.1 | `Migration.Closed`, `Migration.V.restrict`, `Migration.restrict_spec`, `Migration.select`, `Migration.select_incl`, `Migration.select_incl_injective`, `Migration.select_incl_preserves_names`, `Migration.select_undefined` |
| Definition 16.2 | `Migration.lookup`, `Migration.lookup_unique`, `Migration.lookup_none`, `Migration.keyViolations_sub` |
| Definition 16.3 | `Migration.ExtMapping`, `Migration.ExtMapping.delta`, `Migration.toExt`, `Migration.toExt_delta` |
| Proposition 16.4 | `Migration.fragment`, `Migration.fragment_label_origin`, `Migration.fragment_one_origin`, `Migration.fragment_undefined` |
| Definition 16.5 | `Migration.coprodSchema`, `Migration.coprod`, `Migration.inlMapping`, `Migration.inrMapping`, `Migration.coprod_inl`, `Migration.coprod_inr` |
| Proposition 16.6 | `Migration.history_through_migration`, `Migration.map_injective`, `Migration.ref_map` |
| Definition 17.1; proposition 17.2 | `Views.templates`, `Views.Tmpl.inst`, `Views.rdf`, `Views.templates_mapRefs`, `Views.Tmpl.inst_mono`, `Views.rdf_monotone` |
| Proposition 17.3 | `Views.rdfInv`, `Views.RoundTrip`, `Views.rdf_round_trip` |
| Corollary 17.4 | `Views.shacl_min_count`, `Views.shacl_max_count`, `Views.shacl_datatype`, `Views.shacl_in`, `Views.shacl_class`, `Views.shacl_class_edge` |
| Definition 18.1 | `Views.gql`, `Views.gqlOf`, `Views.gNodeTmpl`, `Views.gEdgeTmpls`, `Views.gType`, `Views.mgqlGraph`, `Views.mgqlType` |
| Proposition 18.2 | `Views.NodeConforms`, `Views.EdgeConforms`, `Views.gql_conforms`, `Views.mgql_conforms` |
| Proposition 18.3 | `Views.gql_monotone` |
| Proposition 18.4 | `Views.gql_of_rdf` |
| Section 19 | `Oracle.Aviation.instances`, `Oracle.Gen5.instances`, `Oracle.exportView`, `Oracle.readNQuads`, `Oracle.Corpus.failures`, `Oracle.corpora` |

Section 19's oracle runs as `lake exe apg_oracle <dir>` (`ApgModel/Oracle/Main.lean`), writing each corpus set's
N-Quads once `Corpus.failures` finds nothing; the build checks sets 0 to 3 of each schema. Stage 6.6's group 9 reads the
files it writes, and stage 7's differential queries read the GQL view's projection onto MGQL (proposition 18.2).

## The paper's Coq development

`APG.v` of `github.com/CategoricalData/APG`, commit `7190194`, against this model, statement by statement where it has
a counterpart:

| `SCHEMA-APG.md` | `APG.v` | Lean | Comparison |
|---|---|---|---|
| Definition 3.2 | `Ty L`: `Zero`, `One`, `Plus`, `Times`, `Base`, `Label` | `Core.Ty` | The same constructors; Lean adds `list` and `set` (definition 12.1) |
| Definition 4.1 | `schema`: `L0`, `SIGMA` | `Core.Schema` | The same, the labels a parameter |
| Definitions 5.1, 5.2 | `TERM`, `tyToSet`, `MapTERM` | `Core.V`, `Core.V.map` | `APG.v` types values by the inductive `TERM` and reads them as sets by `tyToSet`; Lean computes the type of values by recursion on the type, which is both (decision L1). `MapTERM` is `V.map` |
| Definition 6.1 | `apg`: `E0`, `PHI` | `Core.Apg`: `E`, `υ` | The same shape |
| Definition 7.1 | `apgMorphism`: `LMAP`, `PFNAT`; `apgMorphismEq` | `Core.Hom`: `h`, `comm`; `Hom.ext` | The same; `apgMorphismEq` uses `proof_irrelevance`, an axiom in Coq, which Lean has by definition |
| Proposition 7.2 | `IdapgMorphism`, `apgMorphismCompose`, `apgInst` | `Hom.id`, `Hom.comp`, `Hom.id_comp`, `Hom.comp_id`, `Hom.comp_assoc` | `APG.v` builds the category record; Lean states its laws as equations, with no category library |
| Definitions 8.5, 8.6 | point-free `term`, `termToFn` | `Core.Tm`, `Core.Tm.eval` | Lean's terms bind variables, so a `case` reads its context, which distributivity needs (decision L3) |
| Definition 8.2 | `freeBCC`: `term` modulo `termEq`, quotiented by the axiom `axiomQ` | evaluation | `APG.v`'s is the free bicartesian category, without the distributive law of `SCHEMA-APG.md`'s `C_S`; Lean equates terms by their evaluation |
| Proposition 8.4 | `apgToFunctor`, `functorToAPG`, `apgIsBc` | `Model.ofApg`, `Model.toApg`, `Model.toApg_ofApg`, `Model.ofApg_toApg`, `Model.interp_eq_eval` | `APG.v` builds both directions and proves no round trip; Lean proves both, for the distributive theory in generator form (decision L4) |
| Definition 9.1 | `apgMapping`: `oMap`, `aMap`; `MapTy` | `Core.Mapping`: `τ`, `t`; `Core.Ty.subst` | The same shape, `aMap` point-free and `t` a term in one variable |
| Proposition 9.3 | `apgMtoF`, `apgMappingIsBc`, `apgFtoM` | `Tm.along`, `Tm.eval_along`; `Mapping.delta`, `Mapping.deltaHom_id`, `Mapping.deltaHom_comp`, `Mapping.delta_id`, `Mapping.delta_comp` | `APG.v` has a mapping's functor between theories and its way back; `Δ` and its functoriality are Lean's alone |
| none | `Initialapg`, `apgInstInitial`, `CoProductapg`, `apgHasCoProducts` | none | The initial APG and the coproduct of two APGs on one schema; definition 16.5's coproduct is of schemas, `Migration.coprod`, which `APG.v` lacks |
| Definition 12.4's laws | `caml.v`: `ExpEq`, `axiomsSound` | `Tm.eval_map_id`, `Tm.eval_map_map` and their set forms | `caml.v` proves a syntactic theory of lists sound; Lean proves definition 12.4's laws by evaluation |

Propositions 3.3, 5.3, 6.3, 6.6, 8.1, 8.7, 9.4 and 9.5, part 2's results and part 3 have no counterpart in `APG.v`.

`APG.v` assumes functional extensionality, proof irrelevance, `JMeq`'s axiom, classical epsilon and `axiomQ`, a
quotient of every setoid; `caml.v` functional, set and stream extensionality and decidable equality of every type.
Lean has quotients built in, functional extensionality from `Quot.sound` and proof irrelevance by definition, and the
model proves each decidable equality it uses.
-/

namespace ApgModel

/-- Corollary 12.3: with collections, values stay functorial in the family (proposition 5.3, for `list` and `set`
types as for the others), and a label-free type's values are the same over every family (proposition 12.2), so each
label-free subterm of `σ(l)` contributes a constant to `F_S`. Propositions 6.3, 6.6, 7.2, 8.1 and 8.4 are proved over
the types with collections, unchanged. -/
theorem corollary_12_3 {T : Typeside} {L : Type} :
    (∀ (E : L → Type) (t : Core.Ty T.P L) (x : Core.V T E t), Core.V.map (fun _ y => y) t x = x) ∧
    (∀ (E E' E'' : L → Type) (h : ∀ l, E l → E' l) (h' : ∀ l, E' l → E'' l) (t : Core.Ty T.P L)
      (x : Core.V T E t), Core.V.map (fun l y => h' l (h l y)) t x = Core.V.map h' t (Core.V.map h t x)) ∧
    (∀ (E E' : L → Type) (t : Core.Ty T.P L), t.LabelFree → Core.V T E t = Core.V T E' t) :=
  ⟨fun _ t x => Core.V.map_id t x, fun _ _ _ h h' t x => Core.V.map_comp h h' t x,
    fun E E' t ht => Core.V_labelFree E E' t ht⟩

/-! ## Every name of the table -/

example := @corollary_12_3
example := @Typeside
example := @Core.Ty
example := @Core.Schema
example := @Core.Ty.Admitted
example := @Named.Name
example := @Named.NTy
example := @Named.NTy.WF
example := @Named.NSchema.IsVertex
example := @Named.NSchema.IsEdge
example := @Named.NSchema.IsVertexProperty
example := @Named.NSchema.IsEdgeProperty
example := @Named.NSchema.IsMetaEdge
example := @Named.NSchema.IsHyperedge
example := @Named.toBin
example := @Named.ofBin
example := @Named.ofBin_toBin
example := @Named.toBin_ofBin
example := @Named.toBin_map
example := @Named.NApg.toCore
example := @Named.NApg.ofCore
example := @Named.NApg.ofCore_toCore
example := @Named.NApg.toCore_ofCore
example := @Named.NApg.comm_iff
example := @Named.NTm.bin
example := @Named.NTm.eval_bin
example := @Named.recAt
example := @Named.projAt
example := @Named.injAt
example := @Named.caseAt
example := @Named.recAt_eval
example := @Named.projAt_eval
example := @Named.injAt_eval
example := @Named.caseAt_eval
example := @Core.V
example := @Core.V.map
example := @Core.V.map_id
example := @Core.V.map_comp
example := @Named.VTerm
example := @Named.denote
example := @Named.check
example := @Named.check_iff
example := @Named.denote_complete
example := @Named.VTerm.refs
example := @Named.denote_refs
example := @Core.Apg
example := @Core.SingleSet
example := @Core.toSingleSet
example := @Core.ofSingleSet
example := @Core.ofSingleSet_toSingleSet
example := @Core.toSingleSet_ofSingleSet_elements
example := @Core.toSingleSet_ofSingleSet_lam
example := @Core.toSingleSet_ofSingleSet_value
example := @Named.FinApg
example := @Named.FinApg.valid
example := @Named.FinApg.toNApg
example := @Core.Enumerable
example := @Core.V.Ref
example := @Core.Apg.Arc
example := @Core.SingleSet.Arc
example := @Core.SingleSet.arc_target_label
example := @Core.Apg.arc_target
example := @Examples.apg
example := @Examples.apg_valid
example := @Examples.a1_references_m1
example := @Examples.misfiled_invalid
example := @Examples.duplicated_invalid
example := @Core.Hom
example := @Core.Hom.id
example := @Core.Hom.comp
example := @Core.Hom.id_comp
example := @Core.Hom.comp_id
example := @Core.Hom.comp_assoc
example := @Core.Iso
example := @Core.Coalg
example := @Core.Coalg.Hom
example := @Core.Apg.toCoalg
example := @Core.Coalg.toApg
example := @Core.Coalg.toApg_toCoalg
example := @Core.Apg.toCoalg_toApg
example := @Core.Hom.toCoalg
example := @Core.Coalg.Hom.toApg
example := @Core.Hom.toCoalg_toApg
example := @Core.Coalg.Hom.toApg_toCoalg
example := @Core.Model
example := @Core.Model.ofApg
example := @Core.Model.toApg
example := @Core.Model.interp_eq_eval
example := @Core.Model.toApg_ofApg
example := @Core.Model.ofApg_toApg
example := @Core.Tm.eval_hom
example := @Core.Model.Hom.natural
example := @Core.Tm
example := @Core.Tm.eval
example := @Core.Tm.eval_terminates
example := @Core.cyclicApg
example := @Core.Mapping
example := @Core.Ty.subst
example := @Core.iota
example := @Core.iotaInv
example := @Core.Mapping.delta
example := @Core.Mapping.deltaHom
example := @Core.Mapping.deltaHom_id
example := @Core.Mapping.deltaHom_comp
example := @Core.Mapping.id
example := @Core.Mapping.delta_id
example := @Core.Tm.along
example := @Core.Tm.eval_along
example := @Core.Mapping.comp
example := @Core.Mapping.delta_comp
example := @Core.V_enumerable
example := @Core.Mapping.delta_finite
example := @Core.labelSum
example := @Core.labelSumBij
example := @Core.Mapping.delta_label_image
example := @Core.Mapping.delta_one_origin
example := @Examples.migration
example := @Examples.migrated
example := @Examples.migrated_memberships
example := @Examples.m1_collection
example := @Examples.m1_member
example := @Examples.m1_role
example := @Examples.a1_names_m1
example := @Core.Ty.LabelFree
example := @Core.V_labelFree
example := @Core.V_labelFree_empty
example := @Core.V.map_labelFree
example := @Core.V.map_labelFree_self
example := @Core.Tm.map
example := @Core.Tm.filter
example := @Core.Tm.mapSet
example := @Core.Tm.filterSet
example := @Core.Tm.DerefFree
example := @Core.Tm.eval_map_id
example := @Core.Tm.eval_map_map
example := @Core.Tm.eval_mapSet_id
example := @Core.Tm.eval_mapSet_mapSet
example := @Named.roleLabelType
example := @Named.symNav
example := @Named.symNav_symm
example := @Named.unnest
example := @Named.unnest_origin
example := @Catalog.Catalog
example := @Catalog.Catalog.conc
example := @Catalog.Catalog.schema
example := @Catalog.Catalog.ranges
example := @Catalog.inRanges_compress
example := @Catalog.Catalog.inRanges_iff_isSub
example := @Catalog.Catalog.is_iff_inRanges
example := @Catalog.injViolations
example := @Catalog.violators
example := @Catalog.cycleViolations
example := @Catalog.keyViolations
example := @Catalog.keyViolations_nil_iff
example := @Catalog.uniqueViolations
example := @Catalog.uniqueViolations_nil_iff
example := @Catalog.nonEmptyViolations
example := @Catalog.nonEmptyViolations_nil_iff
example := @Catalog.setViolations
example := @Catalog.setViolations_nil_iff
example := @Catalog.roleLabelViolations
example := @Catalog.roleLabelViolations_nil_iff
example := @Catalog.participationViolations
example := @Catalog.participationViolations_nil_iff
example := @Catalog.checkViolations
example := @Catalog.checkViolations_nil_iff
example := @Catalog.acyclicViolations
example := @Catalog.acyclicViolations_nil_iff
example := @Catalog.maxDepthViolations
example := @Catalog.maxDepthViolations_nil_iff
example := @Catalog.noOverlapViolations
example := @Catalog.noOverlapViolations_nil_iff
example := @Catalog.limitViolations
example := @Catalog.limitViolations_nil_iff
example := @Catalog.limitGroupViolations
example := @Catalog.limitGroupViolations_nil_iff
example := @Catalog.State
example := @Catalog.State.LabelFixed
example := @Catalog.State.FieldsFixed
example := @Catalog.State.AppendOnly
example := @Catalog.State.Bitemporal
example := @Catalog.State.OnDeleteHolds
example := @Catalog.State.OnRetractHolds
example := @Catalog.State.RetractsHolds
example := @Catalog.State.EndsHolds
example := @Catalog.State.LiveParticipants
example := @Views.ident
example := @History.Exclusive
example := @History.naming_injective
example := @History.union
example := @History.union_closed
example := @History.HState
example := @History.Kinds
example := @History.Live
example := @History.Bel
example := @History.LiveRead
example := @History.BelRead
example := @History.Inv
example := @History.Step
example := @History.Reachable
example := @History.inv_step
example := @History.inv_reachable
example := @History.live_closed
example := @History.believed_closed
example := @History.liveRead_closed
example := @History.belRead_closed
example := @History.prop_15_2
example := @Migration.Closed
example := @Migration.V.restrict
example := @Migration.restrict_spec
example := @Migration.select
example := @Migration.select_incl
example := @Migration.select_incl_injective
example := @Migration.select_incl_preserves_names
example := @Migration.select_undefined
example := @Migration.lookup
example := @Migration.lookup_unique
example := @Migration.lookup_none
example := @Migration.keyViolations_sub
example := @Migration.ExtMapping
example := @Migration.ExtMapping.delta
example := @Migration.toExt
example := @Migration.toExt_delta
example := @Migration.fragment
example := @Migration.fragment_label_origin
example := @Migration.fragment_one_origin
example := @Migration.fragment_undefined
example := @Migration.coprodSchema
example := @Migration.coprod
example := @Migration.inlMapping
example := @Migration.inrMapping
example := @Migration.coprod_inl
example := @Migration.coprod_inr
example := @Migration.history_through_migration
example := @Migration.map_injective
example := @Migration.ref_map
example := @Views.templates
example := @Views.Tmpl.inst
example := @Views.rdf
example := @Views.templates_mapRefs
example := @Views.Tmpl.inst_mono
example := @Views.rdf_monotone
example := @Views.rdfInv
example := @Views.RoundTrip
example := @Views.rdf_round_trip
example := @Views.shacl_min_count
example := @Views.shacl_max_count
example := @Views.shacl_datatype
example := @Views.shacl_in
example := @Views.shacl_class
example := @Views.shacl_class_edge
example := @Views.gql
example := @Views.gqlOf
example := @Views.gNodeTmpl
example := @Views.gEdgeTmpls
example := @Views.gType
example := @Views.mgqlGraph
example := @Views.mgqlType
example := @Views.NodeConforms
example := @Views.EdgeConforms
example := @Views.gql_conforms
example := @Views.mgql_conforms
example := @Views.gql_monotone
example := @Views.gql_of_rdf
example := @Oracle.Aviation.instances
example := @Oracle.Gen5.instances
example := @Oracle.exportView
example := @Oracle.readNQuads
example := @Oracle.Corpus.failures
example := @Oracle.corpora

end ApgModel
