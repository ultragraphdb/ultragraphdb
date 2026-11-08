/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Catalog.Constraints
import ApgModel.Views.RdfRoundTrip

/-!
# Graphs, states and slices

`SCHEMA-APG.md` section 15: proposition 15.1, the union of graphs, and proposition 15.2, the closure of the live and
believed slices of every state the history reaches (decision L9 of `openspec/notes/archive/prepare-apg-lean.md`).

**States.** A state holds every element ever written and not purged, each with its label, its versions, newest first,
each the value it held over a transaction-time interval of epochs, and its `asserted` interval (`SCHEMA.md` section
8.4). What a label is, `@bitemporal`, `@append_only`, an entity, and what each of its roles does at a participant's
retraction or deletion, are the catalog's (`Kinds`).

**Slices.** The live slice holds every element present and not retracted, an element that is not `@bitemporal` only
while every participant of it is live (`Live`); the slice believed at `t` holds every `@bitemporal` element whose
`asserted` contains `t` and every other element present while every participant of it is in the slice (`Bel`).

**Commits.** A commit is one step at the next epoch (`Step`), as section 14.3's transition constraints state it:

- an **insert** of a fresh element naming only live elements (`UGS1019`), a `@bitemporal` one only `@bitemporal`
  participants or `@append_only` entities (`UGS0602`);
- a **revision** of an element that is not `@append_only` (`UGS1013`), a new version of a `@bitemporal` one;
- a **retraction** of a set of live `@bitemporal` elements, every live `@bitemporal` element naming one through a role
  retracted with it, by `cascade`, by `restrict open` when its valid interval is closed, or refused (`UGS1020`); a
  `@retracts` role is a reference into history, which no action follows, so an insert through one is an insert
  followed by the retraction of its participant (`SCHEMA.md` section 8.4, rule 7);
- a **deletion** of a set of elements that are neither `@bitemporal` nor `@append_only`, every other element naming
  one doing so through an `orphan` role, which is emptied, the delete otherwise cascading into the set or refused,
  `restrict` counting every instance (`SCHEMA.md` section 8.5).

**Proposition 15.2** (`prop_15_2`): an invariant (`Inv`) holds of the empty state and every step keeps it
(`inv_step`); under it the live slice and the slice believed at every instant are closed under references through
every role but a `@retracts` one (`liveRefs`, `live_closed`, `believed_closed`), and the state itself, the `with
retracted` slice, under every reference (`state_closed`). A read's slice adds what a `@retracts` role reaches, as the
`with retracted` slice holds it (`LiveRead`, `BelRead`), and is closed under every reference (`liveRead_closed`,
`belRead_closed`), so an APG.

**Names** (section 15.1, `naming_injective`): on a valid APG written as the store writes it (proposition 17.3's
conditions) and meeting section 14.2's exclusivity, two elements with one identity, an IRI, a triple or a pair, are one.
A triple names one element because the navigation name and the anchor's label pick its relation and `@set` allows one
element per pair; a pair because a role label allows one element per instance and participant.
-/

namespace ApgModel.History

open Named Catalog

variable {T : Typeside}

/-! ## Proposition 15.1: graphs -/

section Graphs

variable {S : NSchema T.P Name}

/-- The union of graphs: every element of each. -/
def union (gs : List (FinApg S)) : FinApg S := ⟨gs.flatMap FinApg.elems⟩

/-- Proposition 15.1: the union of graphs with disjoint identifiers, each element's references landing in one of them,
has distinct identifiers and is closed under references. -/
theorem union_closed (gs : List (FinApg S)) (hnd : ((union gs).elems.map Prod.fst).Nodup)
    (href : ∀ g ∈ gs, ∀ x ∈ g.elems, ∀ p ∈ x.2.2.refs, ∃ g' ∈ gs, p ∈ g'.ids) :
    ((union gs).ids).Nodup ∧ ∀ x ∈ (union gs).elems, ∀ p ∈ x.2.2.refs, p ∈ (union gs).ids := by
  refine ⟨hnd, fun x hx p hp => ?_⟩
  simp only [union, List.mem_flatMap] at hx
  obtain ⟨g, hg, hx⟩ := hx
  obtain ⟨g', hg', hp'⟩ := href g hg x hx p hp
  simp only [FinApg.ids, union, List.mem_map, List.mem_flatMap] at hp' ⊢
  obtain ⟨y, hy, rfl⟩ := hp'
  exact ⟨y, ⟨g', hg', hy⟩, rfl⟩

end Graphs

/-! ## States -/

/-- What a catalog says of each label: whether it is `@bitemporal`, `@append_only` or an entity type, each role's
actions at a participant's deletion and retraction, and whether a value's valid interval is open. -/
structure Kinds (T : Typeside) where
  /-- The label is `@bitemporal`. -/
  bitemporal : Name → Bool
  /-- The label is `@append_only`. -/
  appendOnly : Name → Bool
  /-- The label is an entity type. -/
  entity : Name → Bool
  /-- A role's `@on_delete` action, by the referencing label and the role. -/
  onDelete : Name → Name → State.OnDelete
  /-- A role's `@on_retract` action. -/
  onRetract : Name → Name → State.OnRetract
  /-- A role is `@retracts`: a reference into history, to the participant its element's insert retracted. -/
  retracts : Name → Name → Bool
  /-- A value's valid interval is open. -/
  isOpen : VTerm T → Bool

/-- A version: the value an element held from the epoch `start`, until `stop` when closed. -/
structure Version (T : Typeside) where
  /-- The value. -/
  value : VTerm T
  /-- The epoch it opened at. -/
  start : Nat
  /-- The epoch it closed at. -/
  stop : Option Nat

/-- An element of a state: its identifier, its label, its versions newest first, and its `asserted` interval. -/
structure HElem (T : Typeside) where
  /-- The identifier. -/
  id : Nat
  /-- The label. -/
  label : Name
  /-- The versions, newest first. -/
  versions : List (Version T)
  /-- The epoch the element was inserted at. -/
  start : Nat
  /-- The epoch it was retracted at. -/
  stop : Option Nat

/-- A state: its elements and the epoch of its last commit. -/
structure HState (T : Typeside) where
  /-- The elements. -/
  elems : List (HElem T)
  /-- The epoch of the last commit. -/
  now : Nat

/-- An element's newest value. -/
def HElem.newest (x : HElem T) : VTerm T :=
  match x.versions with
  | v :: _ => v.value
  | [] => .record []

/-- An interval `[start, stop)` of epochs contains `t`. -/
def contains (start : Nat) (stop : Option Nat) (t : Nat) : Bool := decide (start ≤ t) && stop.all (t < ·)

theorem contains_iff {start : Nat} {stop : Option Nat} {t : Nat} :
    contains start stop t = true ↔ start ≤ t ∧ ∀ b, stop = some b → t < b := by
  cases stop <;> simp [contains]

variable (Kd : Kinds T)

/-- The references a value of the label `l` holds through its roles that are not `@retracts`, the ones its slices
hold to. -/
def liveRefs (l : Name) : VTerm T → List Nat
  | .record fs => (fs.filter fun f => !Kd.retracts l f.1).flatMap (·.2.refs)
  | _ => []

theorem mem_refsFields {fs : List (Name × VTerm T)} {f : Name × VTerm T} (hf : f ∈ fs) {p : Nat}
    (hp : p ∈ f.2.refs) : p ∈ VTerm.refs.refsFields fs := by
  induction fs with
  | nil => cases hf
  | cons g gs ih =>
    simp only [VTerm.refs.refsFields, List.mem_append]
    rcases List.mem_cons.mp hf with rfl | hf
    · exact Or.inl hp
    · exact Or.inr (ih hf)

theorem mem_liveRefs {l : Name} {fs : List (Name × VTerm T)} {p : Nat} :
    p ∈ liveRefs Kd l (.record fs) ↔ ∃ f ∈ fs, Kd.retracts l f.1 = false ∧ p ∈ f.2.refs := by
  simp [liveRefs, List.mem_flatMap, List.mem_filter, and_assoc]

theorem liveRefs_sub {l : Name} {v : VTerm T} {p : Nat} (hp : p ∈ liveRefs Kd l v) : p ∈ v.refs := by
  cases v with
  | record fs =>
    obtain ⟨f, hf, _, hpf⟩ := (mem_liveRefs Kd).mp hp
    exact mem_refsFields hf hpf
  | _ => cases hp

/-! ## Slices -/

/-- The live slice: a `@bitemporal` element not retracted, any other element while every participant is live. -/
inductive Live (S : HState T) : Nat → Prop
  | bitemporal {x : HElem T} : x ∈ S.elems → Kd.bitemporal x.label = true → x.stop = none → Live S x.id
  | other {x : HElem T} : x ∈ S.elems → Kd.bitemporal x.label = false → (∀ p ∈ x.newest.refs, Live S p) →
      Live S x.id

/-- The slice believed at `t`: a `@bitemporal` element whose `asserted` contains `t`, any other element while every
participant is in the slice. -/
inductive Bel (S : HState T) (t : Nat) : Nat → Prop
  | bitemporal {x : HElem T} : x ∈ S.elems → Kd.bitemporal x.label = true → contains x.start x.stop t = true →
      Bel S t x.id
  | other {x : HElem T} : x ∈ S.elems → Kd.bitemporal x.label = false → (∀ p ∈ x.newest.refs, Bel S t p) →
      Bel S t x.id

/-- An element's values believed at `t`: the versions current at `t` of a `@bitemporal` element, the newest value of
any other. -/
def valuesAt (x : HElem T) (t : Nat) : List (VTerm T) :=
  if Kd.bitemporal x.label then (x.versions.filter fun v => contains v.start v.stop t).map Version.value
  else [x.newest]

/-! ## The invariant -/

/-- What every reachable state holds. -/
structure Inv (S : HState T) : Prop where
  /-- Identifiers are distinct. -/
  nodup : (S.elems.map HElem.id).Nodup
  /-- Every value is a record. -/
  records : ∀ x ∈ S.elems, ∀ v ∈ x.versions, ∃ fs, v.value = .record fs
  /-- An entity names nothing. -/
  entities : ∀ x ∈ S.elems, Kd.entity x.label = true → ∀ v ∈ x.versions, v.value.refs = []
  /-- Each version of a `@bitemporal` element names, over its interval and through its roles that are not
  `@retracts`, `@bitemporal` elements asserted throughout it, or `@append_only` entities that are not `@bitemporal`. -/
  bitemp : ∀ x ∈ S.elems, Kd.bitemporal x.label = true → ∀ v ∈ x.versions, ∀ p ∈ liveRefs Kd x.label v.value,
    ∃ y ∈ S.elems,
    y.id = p ∧ ((Kd.bitemporal y.label = true ∧
        ∀ t, contains v.start v.stop t = true → contains y.start y.stop t = true) ∨
      (Kd.bitemporal y.label = false ∧ Kd.appendOnly y.label = true ∧ Kd.entity y.label = true))
  /-- Every newest value names present elements. -/
  closed : ∀ x ∈ S.elems, ∀ p ∈ x.newest.refs, ∃ y ∈ S.elems, y.id = p
  /-- An element that is not `@bitemporal` is never retracted. -/
  others : ∀ x ∈ S.elems, Kd.bitemporal x.label = false → x.stop = none
  /-- Every epoch an element records lies at or before the state's. -/
  times : ∀ x ∈ S.elems, x.start ≤ S.now ∧ (∀ b, x.stop = some b → b ≤ S.now) ∧
    ∀ v ∈ x.versions, ∀ b, v.stop = some b → b ≤ S.now
  /-- A `@bitemporal` element's older versions are closed, and its newest is open exactly while it is live. -/
  shape : ∀ x ∈ S.elems, Kd.bitemporal x.label = true → (∀ v ∈ x.versions.tail, v.stop.isSome = true) ∧
    ∀ v ∈ x.versions.head?, (v.stop = none ↔ x.stop = none)

/-- The empty state holds the invariant. -/
theorem inv_empty : Inv Kd (⟨[], 0⟩ : HState T) :=
  ⟨List.nodup_nil, nofun, nofun, nofun, nofun, nofun, nofun, nofun⟩

/-! ## Proposition 15.2 under the invariant -/

theorem eq_of_id_list {l : List (HElem T)} (hnd : (l.map HElem.id).Nodup) {x y : HElem T} (hx : x ∈ l)
    (hy : y ∈ l) (hid : x.id = y.id) : x = y := by
  induction l with
  | nil => cases hx
  | cons z zs ih =>
    rw [List.map_cons, List.nodup_cons] at hnd
    cases hx with
    | head =>
      cases hy with
      | head => rfl
      | tail _ hy => exact absurd (hid ▸ List.mem_map_of_mem HElem.id hy) hnd.1
    | tail _ hx =>
      cases hy with
      | head => exact absurd (hid.symm ▸ List.mem_map_of_mem HElem.id hx) hnd.1
      | tail _ hy => exact ih hnd.2 hx hy

theorem eq_of_id {S : HState T} (h : Inv Kd S) {x y : HElem T} (hx : x ∈ S.elems) (hy : y ∈ S.elems)
    (hid : x.id = y.id) : x = y :=
  eq_of_id_list h.nodup hx hy hid

theorem newest_mem {x : HElem T} {p : Nat} (hp : p ∈ x.newest.refs) :
    ∃ v ∈ x.versions, x.versions.head? = some v ∧ x.newest = v.value := by
  unfold HElem.newest at hp ⊢
  cases hv : x.versions with
  | nil => simp [hv, VTerm.refs, VTerm.refs.refsFields] at hp
  | cons v vs => exact ⟨v, List.mem_cons_self _ _, rfl, rfl⟩

theorem live_cases {S : HState T} {e : Nat} (hl : Live Kd S e) : ∃ x ∈ S.elems, x.id = e ∧
    ((Kd.bitemporal x.label = true ∧ x.stop = none) ∨
      (Kd.bitemporal x.label = false ∧ ∀ p ∈ x.newest.refs, Live Kd S p)) := by
  cases hl with
  | bitemporal hx hb hs => exact ⟨_, hx, rfl, Or.inl ⟨hb, hs⟩⟩
  | other hx hb hall => exact ⟨_, hx, rfl, Or.inr ⟨hb, hall⟩⟩

theorem bel_cases {S : HState T} {t e : Nat} (hl : Bel Kd S t e) : ∃ x ∈ S.elems, x.id = e ∧
    ((Kd.bitemporal x.label = true ∧ contains x.start x.stop t = true) ∨
      (Kd.bitemporal x.label = false ∧ ∀ p ∈ x.newest.refs, Bel Kd S t p)) := by
  cases hl with
  | bitemporal hx hb hs => exact ⟨_, hx, rfl, Or.inl ⟨hb, hs⟩⟩
  | other hx hb hall => exact ⟨_, hx, rfl, Or.inr ⟨hb, hall⟩⟩

/-- An `@append_only` entity that is not `@bitemporal` is in every slice: it names nothing. -/
theorem entity_refs {S : HState T} (h : Inv Kd S) {y : HElem T} (hy : y ∈ S.elems) (he : Kd.entity y.label = true) :
    y.newest.refs = [] := by
  cases hv : y.versions with
  | nil => simp [HElem.newest, hv, VTerm.refs, VTerm.refs.refsFields]
  | cons v vs =>
    have := h.entities y hy he v (by rw [hv]; exact List.mem_cons_self _ _)
    simpa [HElem.newest, hv] using this

/-- Proposition 15.2, the live slice: every element of it names only elements of it. -/
theorem live_closed {S : HState T} (h : Inv Kd S) {x : HElem T} (hx : x ∈ S.elems) (hl : Live Kd S x.id) :
    ∀ p ∈ liveRefs Kd x.label x.newest, Live Kd S p := by
  intro p hp
  obtain ⟨x', hx', hid, hcase⟩ := live_cases Kd hl
  have := eq_of_id Kd h hx' hx hid
  subst this
  rcases hcase with ⟨hb, hopen⟩ | ⟨_, hall⟩
  · obtain ⟨v, hv, hhead, hval⟩ := newest_mem (liveRefs_sub Kd hp)
    rw [hval] at hp
    obtain ⟨y, hy, rfl, hcase⟩ := h.bitemp x' hx hb v hv p hp
    rcases hcase with ⟨hby, hcov⟩ | ⟨hby, _, hey⟩
    · refine Live.bitemporal hy hby ?_
      have hvopen : v.stop = none := ((h.shape x' hx hb).2 v (by simp [hhead])).mpr hopen
      cases hys : y.stop with
      | none => rfl
      | some b =>
        have := (contains_iff.mp (hcov (max v.start b)
          (contains_iff.mpr ⟨Nat.le_max_left _ _, fun b' hb' => by rw [hvopen] at hb'; cases hb'⟩))).2 b hys
        omega
    · exact Live.other hy hby (by rw [entity_refs Kd h hy hey]; nofun)
  · exact hall p (liveRefs_sub Kd hp)

/-- Proposition 15.2, the slice believed at `t`: every element of it names, in its values believed at `t`, only
elements of it. -/
theorem believed_closed {S : HState T} (h : Inv Kd S) (t : Nat) {x : HElem T} (hx : x ∈ S.elems)
    (hl : Bel Kd S t x.id) : ∀ v ∈ valuesAt Kd x t, ∀ p ∈ liveRefs Kd x.label v, Bel Kd S t p := by
  intro w hw p hp
  obtain ⟨x', hx', hid, hcase⟩ := bel_cases Kd hl
  have := eq_of_id Kd h hx' hx hid
  subst this
  rcases hcase with ⟨hb, _⟩ | ⟨hb, hall⟩
  · simp only [valuesAt, hb, if_true, List.mem_map, List.mem_filter] at hw
    obtain ⟨v, ⟨hv, hct⟩, rfl⟩ := hw
    obtain ⟨y, hy, rfl, hcase⟩ := h.bitemp x' hx hb v hv p hp
    rcases hcase with ⟨hby, hcov⟩ | ⟨hby, _, hey⟩
    · exact Bel.bitemporal hy hby (hcov t hct)
    · exact Bel.other hy hby (by rw [entity_refs Kd h hy hey]; nofun)
  · simp only [valuesAt, hb, Bool.false_eq_true, if_false, List.mem_singleton] at hw
    subst hw
    exact hall p (liveRefs_sub Kd hp)

/-- The state itself, the `with retracted` slice, is closed under references, a `@retracts` role's included. -/
theorem state_closed {S : HState T} (h : Inv Kd S) {x : HElem T} (hx : x ∈ S.elems) :
    ∀ p ∈ x.newest.refs, ∃ y ∈ S.elems, y.id = p :=
  h.closed x hx

/-- A read's live slice: the live elements, and every element one of them reaches through a `@retracts` role, with
everything that element names, as the `with retracted` slice holds them. -/
inductive LiveRead (S : HState T) : Nat → Prop
  | live {e : Nat} : Live Kd S e → LiveRead S e
  | named {x : HElem T} {p : Nat} : x ∈ S.elems → LiveRead S x.id → p ∈ x.newest.refs → LiveRead S p

/-- A read's slice believed at `t`: the believed elements, and every element a value of one believed at `t` reaches
through a `@retracts` role, with everything that element names, as the `with retracted` slice holds them. -/
inductive BelRead (S : HState T) (t : Nat) : Nat → Prop
  | bel {e : Nat} : Bel Kd S t e → BelRead S t e
  | named {x : HElem T} {v : VTerm T} {p : Nat} : x ∈ S.elems → BelRead S t x.id →
      v ∈ valuesAt Kd x t ∨ v = x.newest → p ∈ v.refs → BelRead S t p

/-- Proposition 15.2, a read's live slice: every element of it names, through every role, present elements of it. -/
theorem liveRead_closed {S : HState T} (h : Inv Kd S) {x : HElem T} (hx : x ∈ S.elems) (hl : LiveRead Kd S x.id) :
    ∀ p ∈ x.newest.refs, LiveRead Kd S p ∧ ∃ y ∈ S.elems, y.id = p :=
  fun _ hp => ⟨LiveRead.named hx hl hp, h.closed x hx _ hp⟩

/-- Proposition 15.2, a read's slice believed at `t`: every element of it names, in its values believed at `t`,
elements of it. -/
theorem belRead_closed {S : HState T} (t : Nat) {x : HElem T} (hx : x ∈ S.elems) (hl : BelRead Kd S t x.id) :
    ∀ v ∈ valuesAt Kd x t, ∀ p ∈ v.refs, BelRead Kd S t p :=
  fun _ hv _ hp => BelRead.named hx hl (Or.inl hv) hp

/-! ## Commits -/

/-- The references a value's fields named `n` hold. -/
def refsVia (v : VTerm T) (n : Name) : List Nat :=
  match v with
  | .record fs => (fs.filter (·.1 == n)).flatMap (·.2.refs)
  | _ => []

/-- A version list with its newest closed at `c`. -/
def closeHead (c : Nat) : List (Version T) → List (Version T)
  | v :: vs => { v with stop := some c } :: vs
  | [] => []

/-- An element revised to `v` at `c`: a new version, the newest closed beneath it when the element is `@bitemporal`,
replaced otherwise. -/
def revised (c : Nat) (v : VTerm T) (x : HElem T) : HElem T :=
  { x with versions := ⟨v, c, none⟩ :: (if Kd.bitemporal x.label then closeHead c x.versions else []) }

/-- An element retracted at `c`: its `asserted` and its newest version closed. -/
def retracted (c : Nat) (x : HElem T) : HElem T := { x with stop := some c, versions := closeHead c x.versions }

/-- A value with every field naming an element of `D` emptied. -/
def clearFields (D : List Nat) : VTerm T → VTerm T
  | .record fs => .record (fs.map fun f => if f.2.refs.any (· ∈ D) then (f.1, State.absent) else f)
  | v => v

/-- An element with the fields of its newest value naming an element of `D` emptied, its `orphan` roles. -/
def orphaned (D : List Nat) (x : HElem T) : HElem T :=
  { x with versions := match x.versions with
    | v :: vs => { v with value := clearFields D v.value } :: vs
    | [] => [] }

/-- What an inserted or revised value meets: a record, naming nothing when the label is an entity type, and naming
only live elements (`UGS1019`), a `@bitemporal` label's only `@bitemporal` elements or `@append_only` entities
(`UGS0602`). -/
def Admissible (S : HState T) (l : Name) (v : VTerm T) : Prop :=
  (∃ fs, v = .record fs) ∧ (Kd.entity l = true → v.refs = []) ∧
  ∀ p ∈ v.refs, ∃ y ∈ S.elems, y.id = p ∧ y.stop = none ∧
    (Kd.bitemporal l = true → Kd.bitemporal y.label = true ∨
      (Kd.appendOnly y.label = true ∧ Kd.entity y.label = true))

/-- A commit, at the next epoch: an insert, a revision, a retraction or a deletion, each as sections 8.4, 8.5 and 14.3
of the specifications state it. -/
inductive Step : HState T → HState T → Prop
  | insert {S : HState T} (e : Nat) (l : Name) (v : VTerm T) (hfresh : ∀ x ∈ S.elems, x.id ≠ e)
      (hv : Admissible Kd S l v) :
      Step S ⟨S.elems ++ [⟨e, l, [⟨v, S.now + 1, none⟩], S.now + 1, none⟩], S.now + 1⟩
  | revise {S : HState T} (e : Nat) (v : VTerm T) {x : HElem T} (hx : x ∈ S.elems) (he : x.id = e)
      (happ : Kd.appendOnly x.label = false) (hopen : x.stop = none) (hv : Admissible Kd S x.label v) :
      Step S ⟨S.elems.map fun y => if y.id = e then revised Kd (S.now + 1) v y else y, S.now + 1⟩
  | retract {S : HState T} (C : List Nat)
      (hC : ∀ x ∈ S.elems, x.id ∈ C → Kd.bitemporal x.label = true ∧ x.stop = none)
      (hact : ∀ z ∈ S.elems, Kd.bitemporal z.label = true → z.stop = none → ∀ n, Kd.retracts z.label n = false →
        ∀ p ∈ refsVia z.newest n, p ∈ C → match Kd.onRetract z.label n with
        | .cascade | .restrict => z.id ∈ C
        | .restrictOpen => Kd.isOpen z.newest = false ∧ z.id ∈ C) :
      Step S ⟨S.elems.map fun y => if y.id ∈ C then retracted (S.now + 1) y else y, S.now + 1⟩
  | delete {S : HState T} (D : List Nat)
      (hD : ∀ x ∈ S.elems, x.id ∈ D → Kd.bitemporal x.label = false ∧ Kd.appendOnly x.label = false)
      (hact : ∀ z ∈ S.elems, z.id ∉ D → ∀ n, ∀ p ∈ refsVia z.newest n, p ∈ D →
        Kd.onDelete z.label n = .orphan ∧ Kd.bitemporal z.label = false ∧ Kd.appendOnly z.label = false) :
      Step S ⟨(S.elems.filter fun y => decide (y.id ∉ D)).map (orphaned D), S.now + 1⟩

/-- The states a history reaches from `S₀`. -/
inductive Reachable (S₀ : HState T) : HState T → Prop
  | init : Reachable S₀ S₀
  | step {S S' : HState T} : Reachable S₀ S → Step Kd S S' → Reachable S₀ S'

/-! ## Every commit keeps the invariant -/

/-- An interval contained in another: what `contains` holds of the first, it holds of the second. -/
theorem contains_open {s a : Nat} (h : s ≤ a) : ∀ t, contains a none t = true → contains s none t = true := by
  intro t ht
  rw [contains_iff] at ht ⊢
  exact ⟨Nat.le_trans h ht.1, fun _ hb => by cases hb⟩

theorem nodup_append_single {α : Type} {l : List α} {a : α} (hl : l.Nodup) (ha : a ∉ l) : (l ++ [a]).Nodup := by
  induction l with
  | nil => exact List.nodup_cons.mpr ⟨(fun h => nomatch h), List.nodup_nil⟩
  | cons x xs ih =>
    rw [List.nodup_cons] at hl
    rw [List.cons_append, List.nodup_cons]
    refine ⟨fun hx => ?_, ih hl.2 fun h => ha (List.mem_cons_of_mem _ h)⟩
    rcases List.mem_append.mp hx with hx | hx
    · exact hl.1 hx
    · rw [List.mem_singleton] at hx
      subst hx
      exact ha (List.mem_cons_self _ _)

theorem inv_insert {S : HState T} (h : Inv Kd S) {e : Nat} {l : Name} {v : VTerm T}
    (hfresh : ∀ x ∈ S.elems, x.id ≠ e) (hv : Admissible Kd S l v) :
    Inv Kd ⟨S.elems ++ [⟨e, l, [⟨v, S.now + 1, none⟩], S.now + 1, none⟩], S.now + 1⟩ := by
  obtain ⟨hrec, hent, hrefs⟩ := hv
  have mem : ∀ {x}, x ∈ S.elems ++ [(⟨e, l, [⟨v, S.now + 1, none⟩], S.now + 1, none⟩ : HElem T)] →
      x ∈ S.elems ∨ x = ⟨e, l, [⟨v, S.now + 1, none⟩], S.now + 1, none⟩ := fun hx => by
    simpa using hx
  have old : ∀ {y}, y ∈ S.elems → y ∈ S.elems ++ [(⟨e, l, [⟨v, S.now + 1, none⟩], S.now + 1, none⟩ : HElem T)] :=
    fun hy => List.mem_append_left _ hy
  /- a value admitted names, over `[now + 1, ∞)`, what the invariant asks -/
  have adm : Kd.bitemporal l = true → ∀ p ∈ v.refs, ∃ y ∈ S.elems, y.id = p ∧
      ((Kd.bitemporal y.label = true ∧ ∀ t, contains (S.now + 1) none t = true → contains y.start y.stop t = true) ∨
        (Kd.bitemporal y.label = false ∧ Kd.appendOnly y.label = true ∧ Kd.entity y.label = true)) := by
    intro hb p hp
    obtain ⟨y, hy, rfl, hstop, hk⟩ := hrefs p hp
    refine ⟨y, hy, rfl, ?_⟩
    cases hby : Kd.bitemporal y.label with
    | true =>
      left
      refine ⟨rfl, ?_⟩
      rw [hstop]
      exact contains_open (Nat.le_succ_of_le (h.times y hy).1)
    | false =>
      right
      rcases hk hb with hk | hk
      · rw [hby] at hk; cases hk
      · exact ⟨rfl, hk⟩
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_⟩
  · rw [List.map_append]
    exact nodup_append_single h.nodup fun ha => by
      obtain ⟨y, hy, hye⟩ := List.mem_map.mp ha
      exact hfresh y hy hye
  · intro x hx w hw
    rcases mem hx with hx | rfl
    · exact h.records x hx w hw
    · simp only [List.mem_singleton] at hw; subst hw; exact hrec
  · intro x hx hxe w hw
    rcases mem hx with hx | rfl
    · exact h.entities x hx hxe w hw
    · simp only [List.mem_singleton] at hw; subst hw; exact hent hxe
  · intro x hx hxb w hw p hp
    rcases mem hx with hx | rfl
    · obtain ⟨y, hy, hyp, hk⟩ := h.bitemp x hx hxb w hw p hp
      exact ⟨y, old hy, hyp, hk⟩
    · simp only [List.mem_singleton] at hw
      subst hw
      obtain ⟨y, hy, hyp, hk⟩ := adm hxb p (liveRefs_sub Kd hp)
      exact ⟨y, old hy, hyp, hk⟩
  · intro x hx p hp
    rcases mem hx with hx | rfl
    · obtain ⟨y, hy, hyp⟩ := h.closed x hx p hp
      exact ⟨y, old hy, hyp⟩
    · obtain ⟨y, hy, hyp, _⟩ := hrefs p hp
      exact ⟨y, old hy, hyp⟩
  · intro x hx hxb
    rcases mem hx with hx | rfl
    · exact h.others x hx hxb
    · rfl
  · intro x hx
    rcases mem hx with hx | rfl
    · obtain ⟨h1, h2, h3⟩ := h.times x hx
      exact ⟨Nat.le_succ_of_le h1, fun b hb => Nat.le_succ_of_le (h2 b hb),
        fun w hw b hb => Nat.le_succ_of_le (h3 w hw b hb)⟩
    · refine ⟨Nat.le_refl _, fun b hb => (by simp at hb), fun w hw b hb => ?_⟩
      simp only [List.mem_singleton] at hw
      subst hw
      simp at hb
  · intro x hx hxb
    rcases mem hx with hx | rfl
    · exact h.shape x hx hxb
    · exact ⟨fun _ hw => (by simp at hw), fun w hw => (by simp at hw; subst hw; simp)⟩

theorem closeHead_mem {c : Nat} {vs : List (Version T)} {w : Version T} (hw : w ∈ closeHead c vs) :
    ∃ w0 ∈ vs, w.value = w0.value ∧ w.start = w0.start ∧
      (w.stop = w0.stop ∨ (vs.head? = some w0 ∧ w.stop = some c)) := by
  cases vs with
  | nil => cases hw
  | cons v vs =>
    simp only [closeHead, List.mem_cons] at hw
    rcases hw with rfl | hw
    · exact ⟨v, List.mem_cons_self _ _, rfl, rfl, Or.inr ⟨rfl, rfl⟩⟩
    · exact ⟨w, List.mem_cons_of_mem _ hw, rfl, rfl, Or.inl rfl⟩

theorem newest_closeHead (c : Nat) (x : HElem T) :
    ({ x with versions := closeHead c x.versions } : HElem T).newest = x.newest := by
  cases hv : x.versions <;> simp [HElem.newest, closeHead, hv]

/-- A shorter interval: what lies in `[a, b')` with `b'` before `b`, or the same interval, lies in `[a, b)`. -/
theorem contains_mono {a : Nat} {b b' : Option Nat} (h : b = none ∨ b' = b) :
    ∀ t, contains a b' t = true → contains a b t = true := by
  intro t ht
  rcases h with rfl | rfl
  · rw [contains_iff] at ht ⊢
    exact ⟨ht.1, fun _ hb => by cases hb⟩
  · exact ht

theorem inv_revise {S : HState T} (h : Inv Kd S) {e : Nat} {v : VTerm T} {x : HElem T} (hx : x ∈ S.elems)
    (he : x.id = e) (hopen : x.stop = none) (hv : Admissible Kd S x.label v) :
    Inv Kd ⟨S.elems.map fun y => if y.id = e then revised Kd (S.now + 1) v y else y, S.now + 1⟩ := by
  obtain ⟨hrec, hent, hrefs⟩ := hv
  let f : HElem T → HElem T := fun y => if y.id = e then revised Kd (S.now + 1) v y else y
  have fid : ∀ y, (f y).id = y.id ∧ (f y).label = y.label ∧ (f y).start = y.start ∧ (f y).stop = y.stop := by
    intro y; simp only [f]; split <;> simp [revised]
  have old : ∀ {y}, y ∈ S.elems → f y ∈ S.elems.map f := fun hy => List.mem_map_of_mem f hy
  have isx : ∀ {y}, y ∈ S.elems → y.id = e → y = x := fun hy hye => eq_of_id Kd h hy hx (hye.trans he.symm)
  /- a reference to `p` by a version over an interval lands in the new state as the same element -/
  have land : ∀ {y : HElem T}, y ∈ S.elems → f y ∈ S.elems.map f ∧ (f y).id = y.id ∧ (f y).label = y.label ∧
      (f y).start = y.start ∧ (f y).stop = y.stop := fun hy => ⟨old hy, fid _⟩
  have adm : Kd.bitemporal x.label = true → ∀ p ∈ v.refs, ∃ y ∈ S.elems, y.id = p ∧
      ((Kd.bitemporal y.label = true ∧ ∀ t, contains (S.now + 1) none t = true → contains y.start y.stop t = true) ∨
        (Kd.bitemporal y.label = false ∧ Kd.appendOnly y.label = true ∧ Kd.entity y.label = true)) := by
    intro hb p hp
    obtain ⟨y, hy, rfl, hstop, hk⟩ := hrefs p hp
    refine ⟨y, hy, rfl, ?_⟩
    cases hby : Kd.bitemporal y.label with
    | true =>
      left
      refine ⟨rfl, ?_⟩
      rw [hstop]
      exact contains_open (Nat.le_succ_of_le (h.times y hy).1)
    | false =>
      right
      rcases hk hb with hk | hk
      · rw [hby] at hk; cases hk
      · exact ⟨rfl, hk⟩
  /- the versions of a revised element -/
  have vers : ∀ {y}, y ∈ S.elems → y.id = e → ∀ w ∈ (f y).versions,
      w = ⟨v, S.now + 1, none⟩ ∨ (Kd.bitemporal y.label = true ∧ w ∈ closeHead (S.now + 1) y.versions) := by
    intro y _ hye w hw
    simp only [f, hye, if_true, revised, List.mem_cons] at hw
    rcases hw with rfl | hw
    · exact Or.inl rfl
    · split at hw
      · next hb => exact Or.inr ⟨hb, hw⟩
      · cases hw
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_⟩
  · rw [List.map_map]
    have : (HElem.id ∘ f) = HElem.id := funext fun y => (fid y).1
    rw [this]
    exact h.nodup
  · intro x' hx' w hw
    obtain ⟨y, hy, rfl⟩ := List.mem_map.mp hx'
    by_cases hye : y.id = e
    · rcases vers hy hye w hw with rfl | ⟨_, hw⟩
      · exact hrec
      · obtain ⟨w0, hw0, hval, _⟩ := closeHead_mem hw
        rw [hval]; exact h.records y hy w0 hw0
    · simp only [hye, if_false] at hw; exact h.records y hy w hw
  · intro x' hx' hxe w hw
    obtain ⟨y, hy, rfl⟩ := List.mem_map.mp hx'
    rw [(fid y).2.1] at hxe
    by_cases hye : y.id = e
    · rcases vers hy hye w hw with rfl | ⟨_, hw⟩
      · exact hent (isx hy hye ▸ hxe)
      · obtain ⟨w0, hw0, hval, _⟩ := closeHead_mem hw
        rw [hval]; exact h.entities y hy hxe w0 hw0
    · simp only [hye, if_false] at hw; exact h.entities y hy hxe w hw
  · intro x' hx' hxb w hw p hp
    obtain ⟨y, hy, rfl⟩ := List.mem_map.mp hx'
    rw [(fid y).2.1] at hxb hp
    by_cases hye : y.id = e
    · have hyx := isx hy hye
      rcases vers hy hye w hw with rfl | ⟨_, hw⟩
      · obtain ⟨z, hz, hzp, hk⟩ := adm (hyx ▸ hxb) p (liveRefs_sub Kd hp)
        obtain ⟨hzm, hzid, hzl, hzs, hzt⟩ := land hz
        exact ⟨f z, hzm, hzid.trans hzp, by rw [hzl, hzs, hzt]; exact hk⟩
      · obtain ⟨w0, hw0, hval, hst, hstop⟩ := closeHead_mem hw
        rw [hval] at hp
        obtain ⟨z, hz, hzp, hk⟩ := h.bitemp y hy hxb w0 hw0 p hp
        obtain ⟨hzm, hzid, hzl, hzs, hzt⟩ := land hz
        refine ⟨f z, hzm, hzid.trans hzp, ?_⟩
        rw [hzl, hzs, hzt]
        rcases hk with ⟨hzb, hcov⟩ | hk
        · refine Or.inl ⟨hzb, fun t ht => hcov t ?_⟩
          rw [hst] at ht
          refine contains_mono ?_ t ht
          rcases hstop with hs | ⟨hhead, _⟩
          · exact Or.inr hs
          · left
            subst hyx
            exact ((h.shape y hy hxb).2 w0 (by simp [hhead])).mpr hopen
        · exact Or.inr hk
    · simp only [hye, if_false] at hw
      obtain ⟨z, hz, hzp, hk⟩ := h.bitemp y hy hxb w hw p hp
      obtain ⟨hzm, hzid, hzl, hzs, hzt⟩ := land hz
      exact ⟨f z, hzm, hzid.trans hzp, by rw [hzl, hzs, hzt]; exact hk⟩
  · intro x' hx' p hp
    obtain ⟨y, hy, rfl⟩ := List.mem_map.mp hx'
    by_cases hye : y.id = e
    · simp only [f, hye, if_true, revised, HElem.newest] at hp
      obtain ⟨z, hz, hzp, _⟩ := hrefs p hp
      exact ⟨f z, old hz, (fid z).1.trans hzp⟩
    · simp only [hye, if_false] at hp
      obtain ⟨z, hz, hzp⟩ := h.closed y hy p hp
      exact ⟨f z, old hz, (fid z).1.trans hzp⟩
  · intro x' hx' hxb
    obtain ⟨y, hy, rfl⟩ := List.mem_map.mp hx'
    rw [(fid y).2.1] at hxb
    rw [(fid y).2.2.2]
    exact h.others y hy hxb
  · intro x' hx'
    obtain ⟨y, hy, rfl⟩ := List.mem_map.mp hx'
    obtain ⟨h1, h2, h3⟩ := h.times y hy
    refine ⟨by rw [(fid y).2.2.1]; exact Nat.le_succ_of_le h1,
      fun b hb => by rw [(fid y).2.2.2] at hb; exact Nat.le_succ_of_le (h2 b hb), fun w hw b hb => ?_⟩
    by_cases hye : y.id = e
    · rcases vers hy hye w hw with rfl | ⟨_, hw⟩
      · cases hb
      · obtain ⟨w0, hw0, _, _, hstop⟩ := closeHead_mem hw
        rcases hstop with hs | ⟨_, hs⟩
        · rw [hs] at hb; exact Nat.le_succ_of_le (h3 w0 hw0 b hb)
        · rw [hs] at hb; cases hb; exact Nat.le_refl _
    · simp only [hye, if_false] at hw; exact Nat.le_succ_of_le (h3 w hw b hb)
  · intro x' hx' hxb
    obtain ⟨y, hy, rfl⟩ := List.mem_map.mp hx'
    rw [(fid y).2.1] at hxb
    by_cases hye : y.id = e
    · have hyx := isx hy hye
      subst hyx
      simp only [f, hye, if_true, revised, hxb, List.tail_cons, List.head?_cons, Option.mem_def,
        Option.some.injEq, forall_eq', hopen, iff_true]
      refine ⟨fun w hw => ?_, trivial⟩
      obtain ⟨w0, hw0, _, _, hstop⟩ := closeHead_mem hw
      cases hvs : y.versions with
      | nil => rw [hvs] at hw; cases hw
      | cons u us =>
        rw [hvs] at hw
        simp only [closeHead, List.mem_cons] at hw
        rcases hw with rfl | hw
        · rfl
        · exact (h.shape y hy hxb).1 w (by rw [hvs]; exact hw)
    · simp only [hye, if_false]
      exact h.shape y hy hxb

/-- Every action retracts a live `@bitemporal` element naming a retracted one with it. -/
theorem retract_with {C : List Nat} {z : HElem T} {n : Name}
    (h : match Kd.onRetract z.label n with
      | .cascade | .restrict => z.id ∈ C
      | .restrictOpen => Kd.isOpen z.newest = false ∧ z.id ∈ C) : z.id ∈ C := by
  revert h
  cases Kd.onRetract z.label n with
  | cascade => exact id
  | restrict => exact id
  | restrictOpen => exact fun h => h.2

/-- An open version of a `@bitemporal` element is its newest. -/
theorem open_head {S : HState T} (h : Inv Kd S) {z : HElem T} (hz : z ∈ S.elems) (hb : Kd.bitemporal z.label = true)
    {w : Version T} (hw : w ∈ z.versions) (hopen : w.stop = none) : z.versions.head? = some w := by
  cases hvs : z.versions with
  | nil => rw [hvs] at hw; cases hw
  | cons u us =>
    rw [hvs] at hw
    rcases List.mem_cons.mp hw with rfl | hw
    · rfl
    · have := (h.shape z hz hb).1 w (by rw [hvs]; exact hw)
      rw [hopen] at this
      cases this

theorem inv_retract {S : HState T} (h : Inv Kd S) {C : List Nat}
    (hC : ∀ x ∈ S.elems, x.id ∈ C → Kd.bitemporal x.label = true ∧ x.stop = none)
    (hact : ∀ z ∈ S.elems, Kd.bitemporal z.label = true → z.stop = none → ∀ n, Kd.retracts z.label n = false →
      ∀ p ∈ refsVia z.newest n, p ∈ C → match Kd.onRetract z.label n with
      | .cascade | .restrict => z.id ∈ C
      | .restrictOpen => Kd.isOpen z.newest = false ∧ z.id ∈ C) :
    Inv Kd ⟨S.elems.map fun y => if y.id ∈ C then retracted (S.now + 1) y else y, S.now + 1⟩ := by
  let f : HElem T → HElem T := fun y => if y.id ∈ C then retracted (S.now + 1) y else y
  have fid : ∀ y, (f y).id = y.id ∧ (f y).label = y.label ∧ (f y).start = y.start := by
    intro y; simp only [f]; split <;> simp [retracted]
  have fstop : ∀ y, (f y).stop = if y.id ∈ C then some (S.now + 1) else y.stop := by
    intro y; simp only [f]; split <;> simp [retracted]
  have old : ∀ {y}, y ∈ S.elems → f y ∈ S.elems.map f := fun hy => List.mem_map_of_mem f hy
  have fnew : ∀ y, (f y).newest = y.newest := by
    intro y; simp only [f]; split
    · exact newest_closeHead _ y
    · rfl
  /- a version of `f y` is a version of `y`, its newest closed when `y` is retracted -/
  have vers : ∀ y, ∀ w ∈ (f y).versions, ∃ w0 ∈ y.versions, w.value = w0.value ∧ w.start = w0.start ∧
      (w.stop = w0.stop ∨ (y.id ∈ C ∧ y.versions.head? = some w0 ∧ w.stop = some (S.now + 1))) := by
    intro y w hw
    by_cases hyc : y.id ∈ C
    · simp only [f, hyc, if_true, retracted] at hw
      obtain ⟨w0, hw0, h1, h2, h3⟩ := closeHead_mem hw
      exact ⟨w0, hw0, h1, h2, by rcases h3 with h3 | h3; exact Or.inl h3; exact Or.inr ⟨hyc, h3⟩⟩
    · simp only [f, hyc, if_false] at hw
      exact ⟨w, hw, rfl, rfl, Or.inl rfl⟩
  /- a retracted element's versions are all closed -/
  have closedAfter : ∀ z ∈ S.elems, Kd.bitemporal z.label = true → z.id ∈ C → ∀ w ∈ (f z).versions,
      w.stop.isSome = true := by
    intro z hz hb hzc w hw
    simp only [f, hzc, if_true, retracted] at hw
    cases hvs : z.versions with
    | nil => rw [hvs] at hw; cases hw
    | cons u us =>
      rw [hvs] at hw
      simp only [closeHead, List.mem_cons] at hw
      rcases hw with rfl | hw
      · rfl
      · exact (h.shape z hz hb).1 w (by rw [hvs]; exact hw)
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_⟩
  · rw [List.map_map]
    have : (HElem.id ∘ f) = HElem.id := funext fun y => (fid y).1
    rw [this]
    exact h.nodup
  · intro x' hx' w hw
    obtain ⟨y, hy, rfl⟩ := List.mem_map.mp hx'
    obtain ⟨w0, hw0, hval, _⟩ := vers y w hw
    rw [hval]; exact h.records y hy w0 hw0
  · intro x' hx' hxe w hw
    obtain ⟨y, hy, rfl⟩ := List.mem_map.mp hx'
    obtain ⟨w0, hw0, hval, _⟩ := vers y w hw
    rw [(fid y).2.1] at hxe
    rw [hval]; exact h.entities y hy hxe w0 hw0
  · intro x' hx' hxb w hw p hp
    obtain ⟨z, hz, rfl⟩ := List.mem_map.mp hx'
    rw [(fid z).2.1] at hxb hp
    obtain ⟨w0, hw0, hval, hst, hstop⟩ := vers z w hw
    rw [hval] at hp
    obtain ⟨y, hy, hyp, hk⟩ := h.bitemp z hz hxb w0 hw0 p hp
    refine ⟨f y, old hy, (fid y).1.trans hyp, ?_⟩
    rw [(fid y).2.1, (fid y).2.2, fstop]
    rcases hk with ⟨hyb, hcov⟩ | hk
    · refine Or.inl ⟨hyb, fun t ht => ?_⟩
      rw [hst] at ht
      /- the version's interval shrank or stayed -/
      have ht0 : contains w0.start w0.stop t = true := by
        rcases hstop with hs | ⟨hzc, hhead, hs⟩
        · rw [← hs]; exact ht
        · have hzopen := (hC z hz hzc).2
          have hw0open : w0.stop = none := ((h.shape z hz hxb).2 w0 (by simp [hhead])).mpr hzopen
          rw [hw0open]
          exact contains_mono (Or.inl rfl) t ht
      have hy0 := hcov t ht0
      by_cases hyc : y.id ∈ C
      · simp only [hyc, if_true]
        rw [contains_iff] at hy0 ht ⊢
        refine ⟨hy0.1, fun b hb => ?_⟩
        simp only [Option.some.injEq] at hb
        subst hb
        cases hws : w.stop with
        | some b' =>
          have hb' : b' ≤ S.now + 1 := by
            rcases hstop with hs | ⟨_, _, hs⟩
            · rw [hs] at hws; exact Nat.le_succ_of_le ((h.times z hz).2.2 w0 hw0 b' hws)
            · rw [hs] at hws; cases hws; exact Nat.le_refl _
          exact Nat.lt_of_lt_of_le (ht.2 b' hws) hb'
        | none =>
          /- an open version of a live element naming a retracted one: the element was retracted with it -/
          exfalso
          rcases hstop with hs | ⟨_, _, hs⟩
          · rw [hs] at hws
            have hhead := open_head Kd h hz hxb hw0 hws
            have hzopen : z.stop = none := ((h.shape z hz hxb).2 w0 (by simp [hhead])).mp hws
            obtain ⟨fs, hfs⟩ := h.records z hz w0 hw0
            have hnew : z.newest = w0.value := by
              cases hvs : z.versions with
              | nil => rw [hvs] at hw0; cases hw0
              | cons u us =>
                rw [hvs] at hhead
                simp only [List.head?_cons, Option.some.injEq] at hhead
                subst hhead
                simp [HElem.newest, hvs]
            have hp' : p ∈ liveRefs Kd z.label (VTerm.record fs) := by rw [← hfs]; exact hp
            obtain ⟨g, hg, hgr, hpg⟩ := (mem_liveRefs Kd).mp hp'
            have hn : p ∈ refsVia (VTerm.record fs) g.1 := by
              simp only [refsVia, List.mem_flatMap, List.mem_filter, beq_iff_eq]
              exact ⟨g, ⟨hg, rfl⟩, hpg⟩
            have hzc := retract_with Kd
              (hact z hz hxb hzopen g.1 hgr p (by rw [hnew, hfs]; exact hn) (hyp ▸ hyc))
            have := closedAfter z hz hxb hzc w hw
            rw [← hs] at hws
            rw [hws] at this
            cases this
          · rw [hs] at hws; cases hws
      · simp only [hyc, if_false]
        exact hy0
    · refine Or.inr hk
  · intro x' hx' p hp
    obtain ⟨y, hy, rfl⟩ := List.mem_map.mp hx'
    rw [fnew] at hp
    obtain ⟨z, hz, hzp⟩ := h.closed y hy p hp
    exact ⟨f z, old hz, (fid z).1.trans hzp⟩
  · intro x' hx' hxb
    obtain ⟨y, hy, rfl⟩ := List.mem_map.mp hx'
    rw [(fid y).2.1] at hxb
    rw [fstop]
    split
    · next hyc => rw [(hC y hy hyc).1] at hxb; cases hxb
    · exact h.others y hy hxb
  · intro x' hx'
    obtain ⟨y, hy, rfl⟩ := List.mem_map.mp hx'
    obtain ⟨h1, h2, h3⟩ := h.times y hy
    refine ⟨by rw [(fid y).2.2]; exact Nat.le_succ_of_le h1, fun b hb => ?_, fun w hw b hb => ?_⟩
    · rw [fstop] at hb
      split at hb
      · cases hb; exact Nat.le_refl _
      · exact Nat.le_succ_of_le (h2 b hb)
    · obtain ⟨w0, hw0, _, _, hstop⟩ := vers y w hw
      rcases hstop with hs | ⟨_, _, hs⟩
      · rw [hs] at hb; exact Nat.le_succ_of_le (h3 w0 hw0 b hb)
      · rw [hs] at hb; cases hb; exact Nat.le_refl _
  · intro x' hx' hxb
    obtain ⟨y, hy, rfl⟩ := List.mem_map.mp hx'
    rw [(fid y).2.1] at hxb
    by_cases hyc : y.id ∈ C
    · simp only [f, hyc, if_true, retracted]
      cases hvs : y.versions with
      | nil => simp [closeHead]
      | cons u us =>
        simp only [closeHead, List.tail_cons, List.head?_cons, Option.mem_def, Option.some.injEq, forall_eq',
          reduceCtorEq, iff_false]
        refine ⟨fun w hw => (h.shape y hy hxb).1 w (by rw [hvs]; exact hw), by simp⟩
    · simp only [f, hyc, if_false]
      exact h.shape y hy hxb

/-- Emptying the fields that name `D` keeps the rest of a record's references and drops those. -/
theorem clearFields_refs {D : List Nat} {fs : List (Name × VTerm T)} {p : Nat}
    (hp : p ∈ (clearFields D (.record fs)).refs) : p ∈ (VTerm.record fs).refs ∧ p ∉ D := by
  simp only [clearFields, VTerm.refs] at hp ⊢
  induction fs with
  | nil => simp [VTerm.refs.refsFields] at hp
  | cons f fs ih =>
    obtain ⟨n, w⟩ := f
    by_cases hd : (w.refs.any fun x => decide (x ∈ D)) = true
    · rw [List.map_cons, if_pos hd] at hp
      simp only [VTerm.refs.refsFields, List.mem_append] at hp
      rcases hp with hp | hp
      · simp [State.absent, VTerm.refs, VTerm.refs.refsFields] at hp
      · obtain ⟨h1, h2⟩ := ih hp
        exact ⟨by simp only [VTerm.refs.refsFields, List.mem_append]; exact Or.inr h1, h2⟩
    · rw [List.map_cons, if_neg hd] at hp
      simp only [VTerm.refs.refsFields, List.mem_append] at hp
      simp only [VTerm.refs.refsFields, List.mem_append]
      rcases hp with hp | hp
      · exact ⟨Or.inl hp, fun hD => hd (List.any_eq_true.mpr ⟨p, hp, decide_eq_true hD⟩)⟩
      · obtain ⟨h1, h2⟩ := ih hp
        exact ⟨Or.inr h1, h2⟩

/-- Emptying fields drops references, so a role's references after are among its references before. -/
theorem clearFields_liveRefs {D : List Nat} {l : Name} {fs : List (Name × VTerm T)} {p : Nat}
    (hp : p ∈ liveRefs Kd l (clearFields D (.record fs))) : p ∈ liveRefs Kd l (.record fs) := by
  simp only [clearFields] at hp
  obtain ⟨f', hf', hr, hpf⟩ := (mem_liveRefs Kd).mp hp
  obtain ⟨f, hf, rfl⟩ := List.mem_map.mp hf'
  refine (mem_liveRefs Kd).mpr ⟨f, hf, ?_⟩
  by_cases hd : (f.2.refs.any fun x => decide (x ∈ D)) = true
  · rw [if_pos hd] at hpf
    simp [State.absent, VTerm.refs, VTerm.refs.refsFields] at hpf
  · rw [if_neg hd] at hr hpf
    exact ⟨hr, hpf⟩

theorem newest_orphaned (D : List Nat) (x : HElem T) : (orphaned D x).newest = clearFields D x.newest := by
  cases hvs : x.versions <;> simp [orphaned, HElem.newest, hvs, clearFields]

theorem inv_delete {S : HState T} (h : Inv Kd S) {D : List Nat}
    (hD : ∀ x ∈ S.elems, x.id ∈ D → Kd.bitemporal x.label = false ∧ Kd.appendOnly x.label = false) :
    Inv Kd ⟨(S.elems.filter fun y => decide (y.id ∉ D)).map (orphaned D), S.now + 1⟩ := by
  have mem : ∀ {x'}, x' ∈ (S.elems.filter fun y => decide (y.id ∉ D)).map (orphaned D) →
      ∃ y ∈ S.elems, y.id ∉ D ∧ x' = orphaned D y := by
    intro x' hx'
    obtain ⟨y, hy, rfl⟩ := List.mem_map.mp hx'
    rw [List.mem_filter] at hy
    exact ⟨y, hy.1, of_decide_eq_true hy.2, rfl⟩
  have old : ∀ {y}, y ∈ S.elems → y.id ∉ D →
      orphaned D y ∈ (S.elems.filter fun y => decide (y.id ∉ D)).map (orphaned D) :=
    fun hy hyd => List.mem_map_of_mem _ (List.mem_filter.mpr ⟨hy, decide_eq_true hyd⟩)
  /- an orphaned element's versions: the old ones, the newest's value with its fields naming `D` emptied -/
  have vers : ∀ y : HElem T, ∀ w ∈ (orphaned D y).versions, ∃ w0 ∈ y.versions, w.start = w0.start ∧
      w.stop = w0.stop ∧ (w.value = w0.value ∨ w.value = clearFields D w0.value) := by
    intro y w hw
    cases hvs : y.versions with
    | nil => simp [orphaned, hvs] at hw
    | cons u us =>
      simp only [orphaned, hvs, List.mem_cons] at hw
      rcases hw with rfl | hw
      · exact ⟨u, List.mem_cons_self _ _, rfl, rfl, Or.inr rfl⟩
      · exact ⟨w, List.mem_cons_of_mem _ hw, rfl, rfl, Or.inl rfl⟩
  /- a version's references are among the old version's -/
  have sub : ∀ y ∈ S.elems, ∀ w ∈ (orphaned D y).versions, ∃ w0 ∈ y.versions, w.start = w0.start ∧
      w.stop = w0.stop ∧ (∀ p ∈ w.value.refs, p ∈ w0.value.refs) ∧
      ∀ p ∈ liveRefs Kd y.label w.value, p ∈ liveRefs Kd y.label w0.value := by
    intro y hy w hw
    obtain ⟨w0, hw0, hst, hstop, hval⟩ := vers y w hw
    refine ⟨w0, hw0, hst, hstop, fun p hp => ?_, fun p hp => ?_⟩
    · rcases hval with hval | hval
      · rw [← hval]; exact hp
      · obtain ⟨fs, hfs⟩ := h.records y hy w0 hw0
        rw [hval, hfs] at hp
        rw [hfs]
        exact (clearFields_refs hp).1
    · rcases hval with hval | hval
      · rw [← hval]; exact hp
      · obtain ⟨fs, hfs⟩ := h.records y hy w0 hw0
        rw [hval, hfs] at hp
        rw [hfs]
        exact clearFields_liveRefs Kd hp
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_⟩
  · rw [List.map_map]
    have : (HElem.id ∘ orphaned D : HElem T → Nat) = HElem.id := rfl
    rw [this]
    exact List.Nodup.sublist ((List.filter_sublist _).map _) h.nodup
  · intro x' hx' w hw
    obtain ⟨y, hy, _, rfl⟩ := mem hx'
    obtain ⟨w0, hw0, _, _, hval⟩ := vers y w hw
    obtain ⟨fs, hfs⟩ := h.records y hy w0 hw0
    rcases hval with hval | hval
    · exact ⟨fs, hval.trans hfs⟩
    · exact ⟨_, by rw [hval, hfs]; rfl⟩
  · intro x' hx' hxe w hw
    obtain ⟨y, hy, _, rfl⟩ := mem hx'
    obtain ⟨w0, hw0, _, _, hrefs, _⟩ := sub y hy w hw
    have hnil := h.entities y hy hxe w0 hw0
    cases hr : w.value.refs with
    | nil => rfl
    | cons p ps =>
      have := hrefs p (by rw [hr]; exact List.mem_cons_self _ _)
      rw [hnil] at this
      cases this
  · intro x' hx' hxb w hw p hp
    obtain ⟨y, hy, _, rfl⟩ := mem hx'
    obtain ⟨w0, hw0, hst, hstop, _, hrefs⟩ := sub y hy w hw
    obtain ⟨z, hz, hzp, hk⟩ := h.bitemp y hy hxb w0 hw0 p (hrefs p hp)
    have hzd : z.id ∉ D := fun hzd => by
      have := hD z hz hzd
      rcases hk with ⟨hzb, _⟩ | ⟨_, hza, _⟩
      · rw [this.1] at hzb; cases hzb
      · rw [this.2] at hza; cases hza
    refine ⟨orphaned D z, old hz hzd, hzp, ?_⟩
    rcases hk with ⟨hzb, hcov⟩ | hk
    · exact Or.inl ⟨hzb, fun t ht => hcov t (by rw [← hst, ← hstop]; exact ht)⟩
    · exact Or.inr hk
  · intro x' hx' p hp
    obtain ⟨y, hy, _, rfl⟩ := mem hx'
    rw [newest_orphaned] at hp
    have hnew : ∃ fs, y.newest = .record fs := by
      cases hvs : y.versions with
      | nil => exact ⟨[], by simp [HElem.newest, hvs]⟩
      | cons v vs =>
        obtain ⟨fs, hfs⟩ := h.records y hy v (by rw [hvs]; exact List.mem_cons_self _ _)
        exact ⟨fs, by simp [HElem.newest, hvs, hfs]⟩
    obtain ⟨fs, hfs⟩ := hnew
    rw [hfs] at hp
    obtain ⟨hp', hpd⟩ := clearFields_refs hp
    obtain ⟨z, hz, hzp⟩ := h.closed y hy p (hfs ▸ hp')
    exact ⟨orphaned D z, old hz (hzp ▸ hpd), hzp⟩
  · intro x' hx' hxb
    obtain ⟨y, hy, _, rfl⟩ := mem hx'
    exact h.others y hy hxb
  · intro x' hx'
    obtain ⟨y, hy, _, rfl⟩ := mem hx'
    obtain ⟨h1, h2, h3⟩ := h.times y hy
    refine ⟨Nat.le_succ_of_le h1, fun b hb => Nat.le_succ_of_le (h2 b hb), fun w hw b hb => ?_⟩
    obtain ⟨w0, hw0, _, hstop, _⟩ := vers y w hw
    rw [hstop] at hb
    exact Nat.le_succ_of_le (h3 w0 hw0 b hb)
  · intro x' hx' hxb
    obtain ⟨y, hy, _, rfl⟩ := mem hx'
    have hs := h.shape y hy hxb
    cases hvs : y.versions with
    | nil => simp [orphaned, hvs]
    | cons u us =>
      rw [hvs] at hs
      simpa [orphaned, hvs] using hs

/-- Every commit keeps the invariant. -/
theorem inv_step {S S' : HState T} (h : Inv Kd S) (hs : Step Kd S S') : Inv Kd S' := by
  cases hs with
  | insert e l v hfresh hv => exact inv_insert Kd h hfresh hv
  | revise e v hx he _ hopen hv => exact inv_revise Kd h hx he hopen hv
  | retract C hC hact => exact inv_retract Kd h hC hact
  | delete D hD _ => exact inv_delete Kd h hD

/-- Every state a history reaches from the empty state holds the invariant. -/
theorem inv_reachable {S : HState T} (hr : Reachable Kd ⟨[], 0⟩ S) : Inv Kd S := by
  induction hr with
  | init => exact inv_empty Kd
  | step _ hs ih => exact inv_step Kd ih hs

/-- Proposition 15.2: in every state a history reaches, the live slice, the slice believed at any `t` and the state
itself, the `with retracted` slice, are each closed under references. -/
theorem prop_15_2 {S : HState T} (hr : Reachable Kd ⟨[], 0⟩ S) :
    (∀ x ∈ S.elems, Live Kd S x.id → ∀ p ∈ liveRefs Kd x.label x.newest, Live Kd S p) ∧
    (∀ t, ∀ x ∈ S.elems, Bel Kd S t x.id → ∀ v ∈ valuesAt Kd x t, ∀ p ∈ liveRefs Kd x.label v, Bel Kd S t p) ∧
    (∀ x ∈ S.elems, ∀ p ∈ x.newest.refs, ∃ y ∈ S.elems, y.id = p) :=
  let h := inv_reachable Kd hr
  ⟨fun _ hx hl => live_closed Kd h hx hl, fun t _ hx hl => believed_closed Kd h t hx hl,
    fun _ hx => state_closed Kd h hx⟩

/-! ## Section 15.1: names -/

section Names

open Views

variable [DecidableEq T.P] [∀ p, DecidableEq (T.PV p)] (K : Catalog T)

/-- The exclusivity that lets a triple or a pair name an element (section 14.2): `@set` per pair on each edge-encoded
relation without a reifier, which is no `@multi` relation, every element of the graph counted, and each role label's
per instance and participant. -/
structure Exclusive (G : FinApg K.schema) : Prop where
  /-- No two elements of an edge-encoded relation without a reifier share their anchor and other participant. -/
  triples : ∀ l, tripleNamed K l = true → ∀ a o, K.anchorRole l = some a → K.otherRole l = some o →
    setViolations G (fun _ => true) l [a, o] (K.inherits CType.symmetric l) [] = []
  /-- No two elements of a role label share their instance and participant. -/
  pairs : ∀ l, K.find l = none → roleLabelViolations G l = []

section

variable {K} {vp : ViewParams T} {ord : VTerm T → VTerm T → Bool} {ι : Nat → Option String} {idOf : String → Nat}
  {G : FinApg K.schema}

omit [∀ p, DecidableEq (T.PV p)] in
theorem concrete_of (H : RoundTrip K vp ord ι idOf G) {x : Nat × Name × VTerm T} (hx : x ∈ G.elems) {c : CType T}
    (hc : K.find x.2.1 = some c) : c.concrete = true := by
  rcases H.labels x hx with ⟨c', hc', hconc⟩ | ⟨hnone, _⟩
  · rw [hc] at hc'
    cases hc'
    exact hconc
  · rw [hc] at hnone
    cases hnone

omit [∀ p, DecidableEq (T.PV p)] in
/-- An element named by a triple: the label of its anchor's IRI, which its anchor role admits, and its participants
present. -/
theorem triple_anchor (H : RoundTrip K vp ord ι idOf G) {x : Nat × Name × VTerm T} (hx : x ∈ G.elems)
    {c : CType T} (hc : K.find x.2.1 = some c) (htn : tripleNamed K x.2.1 = true) {a o : Name} {pa po : Nat}
    {s : String} (ha : K.anchorRole x.2.1 = some a) (ho : K.otherRole x.2.1 = some o)
    (hfa : fieldRefs x.2.2 a = [pa]) (hfo : fieldRefs x.2.2 o = [po]) (hs : ι pa = some s) :
    ∃ lpa, labelOf K (rdf K vp ι (fun _ => "") G) s = some lpa ∧ admitsAnchor K x.2.1 lpa = true ∧
      (∃ y ∈ G.elems, y.1 = pa) ∧ ∃ y ∈ G.elems, y.1 = po := by
  have hedge : K.edgeEncoded x.2.1 = true := by
    simp only [tripleNamed, Bool.and_eq_true] at htn
    exact htn.1.2
  obtain ⟨a₁, o₁, _, ha₁, ho₁, _, _, _, _, _, _, _, _, _, _, _, _, _,
    wa, wo, pa₁, po₁, sa, _, lpa, _, hwa, hwo, hra, hro, hsa, _, _, _, hla, _, hadA, _, ⟨za, hza, hza1⟩,
    ⟨zo, hzo, hzo1⟩⟩ := edge_roles K H (fun _ => "") hx hc (concrete_of H hx hc) hedge
  have haa : a₁ = a := Option.some.inj (ha₁.symm.trans ha)
  have hoo : o₁ = o := Option.some.inj (ho₁.symm.trans ho)
  subst haa hoo
  have hpa : pa₁ = pa := by
    rw [fieldRefs_of_field hwa, hra] at hfa
    exact (List.cons.inj hfa).1
  have hpo : po₁ = po := by
    rw [fieldRefs_of_field hwo, hro] at hfo
    exact (List.cons.inj hfo).1
  subst hpa hpo
  have hss : sa = s := Option.some.inj (hsa.symm.trans hs)
  subst hss
  exact ⟨lpa, hla, hadA, ⟨za, hza, hza1⟩, ⟨zo, hzo, hzo1⟩⟩

/-- Section 15.1: the naming is injective where defined. On a valid APG on `S(K)` meeting the round trip's
conditions (proposition 17.3) and the exclusivity of section 14.2, two elements with one identity, an IRI, a triple or
a pair, are one element. -/
theorem naming_injective (H : RoundTrip K vp ord ι idOf G) (E : Exclusive K G) {x y : Nat × Name × VTerm T}
    (hx : x ∈ G.elems) (hy : y ∈ G.elems) {i : Ident} (hxi : ident K ι x = some i) (hyi : ident K ι y = some i) :
    x.1 = y.1 := by
  cases hxc : K.find x.2.1 with
  | some cx =>
    cases hxt : tripleNamed K x.2.1 with
    | false =>
      obtain ⟨s, hs, rfl⟩ := ident_iri K hxc hxt hxi
      cases hyc : K.find y.2.1 with
      | some cy =>
        cases hyt : tripleNamed K y.2.1 with
        | false =>
          obtain ⟨s', hs', heq⟩ := ident_iri K hyc hyt hyi
          cases heq
          exact H.inj x hx y hy s hs hs'
        | true =>
          obtain ⟨_, _, _, _, _, _, _, _, _, _, _, _, _, _, heq⟩ := ident_triple K hyc hyt hyi
          cases heq
      | none =>
        obtain ⟨_, _, _, _, _, _, _, _, heq⟩ := ident_pair K hyc hyi
        cases heq
    | true =>
      obtain ⟨a, o, n, pa, po, s, t, ha, ho, hn, hfa, hfo, hs, ht, rfl⟩ := ident_triple K hxc hxt hxi
      cases hyc : K.find y.2.1 with
      | some cy =>
        cases hyt : tripleNamed K y.2.1 with
        | false =>
          obtain ⟨_, _, heq⟩ := ident_iri K hyc hyt hyi
          cases heq
        | true =>
          obtain ⟨a', o', n', pa', po', s', t', ha', ho', hn', hfa', hfo', hs', ht', heq⟩ :=
            ident_triple K hyc hyt hyi
          simp only [Ident.triple.injEq] at heq
          obtain ⟨rfl, rfl, rfl⟩ := heq
          obtain ⟨lpa, hla, hadA, ⟨za, hza, hza1⟩, ⟨zo, hzo, hzo1⟩⟩ :=
            triple_anchor H hx hxc hxt ha ho hfa hfo hs
          obtain ⟨lpa', hla', hadA', ⟨za', hza', hza1'⟩, ⟨zo', hzo', hzo1'⟩⟩ :=
            triple_anchor H hy hyc hyt ha' ho' hfa' hfo' hs'
          rw [hla] at hla'
          cases hla'
          /- the navigation name and the anchor's label pick the relation -/
          obtain ⟨hxname, hxmem⟩ := find_name K hxc
          obtain ⟨hyname, hymem⟩ := find_name K hyc
          have hedge' : K.edgeEncoded cy.name = true := by
            rw [hyname]
            simp only [tripleNamed, Bool.and_eq_true] at hyt
            exact hyt.1.2
          have hlab : x.2.1 = y.2.1 := by
            have := H.navUnique cx hxmem cy hymem (concrete_of H hx hxc) (concrete_of H hy hyc) (hxname ▸ hxt)
              hedge' n lpa (by rw [hxname, ha]; simpa using hn) (by rw [hyname, ha']; simpa using hn')
              (hxname ▸ hadA) (Or.inl (hyname ▸ hadA'))
            rw [← hxname, ← hyname, this]
          rw [← hlab] at ha' ho'
          have haa : a' = a := Option.some.inj (ha'.symm.trans ha)
          have hoo : o' = o := Option.some.inj (ho'.symm.trans ho)
          subst haa hoo
          have hpa : pa' = pa := by
            subst hza1 hza1'
            exact (H.inj za' hza' za hza s hs' hs)
          have hpo : po' = po := by
            subst hzo1 hzo1'
            exact (H.inj zo' hzo' zo hzo t ht' ht)
          subst hpa hpo
          have hset := (setViolations_nil_iff G (fun _ => true) x.2.1 [a', o'] (K.inherits CType.symmetric x.2.1)
            []).mp (E.triples x.2.1 hxt a' o' ha ho)
          refine Decidable.byContradiction fun hne => hset x (mem_ofLabels.mpr ⟨hx, List.mem_singleton.mpr rfl⟩) y
            (mem_ofLabels.mpr ⟨hy, List.mem_singleton.mpr hlab.symm⟩) rfl rfl hne ⟨?_, fun _ h => nomatch h⟩
          unfold SameRoles
          split
          · intro p
            simp [hfa, hfo, hfa', hfo']
          · simp [hfa, hfo, hfa', hfo']
      | none =>
        obtain ⟨_, _, _, _, _, _, _, _, heq⟩ := ident_pair K hyc hyi
        cases heq
  | none =>
    obtain ⟨pi, pp, s, t, hfi, hfp, hs, ht, rfl⟩ := ident_pair K hxc hxi
    cases hyc : K.find y.2.1 with
    | some cy =>
      cases hyt : tripleNamed K y.2.1 with
      | false =>
        obtain ⟨_, _, heq⟩ := ident_iri K hyc hyt hyi
        cases heq
      | true =>
        obtain ⟨_, _, _, _, _, _, _, _, _, _, _, _, _, _, heq⟩ := ident_triple K hyc hyt hyi
        cases heq
    | none =>
      obtain ⟨pi', pp', s', t', hfi', hfp', hs', ht', heq⟩ := ident_pair K hyc hyi
      simp only [Ident.pair.injEq] at heq
      obtain ⟨hlab, rfl, rfl⟩ := heq
      /- a role label's element is its instance and its participant -/
      have value : ∀ {z : Nat × Name × VTerm T}, z ∈ G.elems → K.find z.2.1 = none → z.2.1 = x.2.1 →
          ∀ {qi qp : Nat}, fieldRefs z.2.2 "instance" = [qi] → fieldRefs z.2.2 "participant" = [qp] →
          ι qi = some s → ι qp = some t →
          ∃ zi ∈ G.elems, ι zi.1 = some s ∧ ∀ rl, K.roleLabelOf x.2.1 = some rl →
            z.2.2 = .record [("instance", .elem zi.1),
              ("participant", participantValue K idOf (rdf K vp ι (fun _ => "") G) rl.2.2.2 t)] := by
        intro z hz hzn hzl qi qp hqi hqp hsi hti
        obtain ⟨rl, hrl⟩ : ∃ rl, K.roleLabelOf z.2.1 = some rl := by
          rcases H.labels z hz with ⟨c', hc', _⟩ | ⟨_, hsome⟩
          · rw [hzn] at hc'
            cases hc'
          · exact Option.isSome_iff_exists.mp hsome
        obtain ⟨zi, wp, hv, _, hwp⟩ := roleLabel_value K H hz hzn hrl
        have hrefs : ∀ p ∈ wp.refs, p ∈ z.2.2.refs := by
          intro p hp
          rw [hv]
          simp [VTerm.refs, VTerm.refs.refsFields, hp]
        obtain ⟨p, sp, hrp, hsp, hpv⟩ :=
          participant_round K H (fun _ => "") hwp fun p hp => H.parts z hz p (hrefs p hp)
        obtain ⟨wi, hwi, hwi1, _, _⟩ := H.parts z hz zi (by rw [hv]; simp [VTerm.refs, VTerm.refs.refsFields])
        have hqi' : qi = zi := by
          rw [hv] at hqi
          exact (List.cons.inj hqi.symm).1
        have hqp' : qp = p := by
          rw [hv] at hqp
          have h2 : wp.refs = [qp] := by simpa [fieldRefs, VTerm.field?] using hqp
          rw [hrp] at h2
          exact (List.cons.inj h2).1.symm
        subst hqi' hqp'
        have hspt : sp = t := Option.some.inj (hsp.symm.trans hti)
        subst hspt
        refine ⟨wi, hwi, hwi1 ▸ hsi, fun rl' hrl' => ?_⟩
        rw [hzl, hrl'] at hrl
        cases hrl
        rw [hv, hpv, hwi1]
      obtain ⟨xi, hxi', hxs, hxv⟩ := value hx hxc rfl hfi hfp hs ht
      obtain ⟨yi, hyi', hys, hyv⟩ := value hy hyc hlab.symm hfi' hfp' hs' ht'
      have hii : xi.1 = yi.1 := H.inj xi hxi' yi hyi' s hxs hys
      obtain ⟨rl, hrl⟩ : ∃ rl, K.roleLabelOf x.2.1 = some rl := by
        rcases H.labels x hx with ⟨c', hc', _⟩ | ⟨_, hsome⟩
        · rw [hxc] at hc'
          cases hc'
        · exact Option.isSome_iff_exists.mp hsome
      have hvv : x.2.2 = y.2.2 := by rw [hxv rl hrl, hyv rl hrl, hii]
      exact (roleLabelViolations_nil_iff G x.2.1).mp (E.pairs x.2.1 hxc) x
        (mem_ofLabels.mpr ⟨hx, List.mem_singleton.mpr rfl⟩) y (mem_ofLabels.mpr ⟨hy, List.mem_singleton.mpr hlab.symm⟩)
        (by rw [hvv])

end

end Names

end ApgModel.History
