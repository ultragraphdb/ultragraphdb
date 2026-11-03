/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/

/-!
# Events, the latest event, and what is visible at an epoch

Every fact the placement model stores is an event appended to a list: an edge event inserts or deletes the edge from a
subject to an entity, and a member event adds, renews or removes a bucket group in an entity's referrer set. Partitions,
subjects, entities, bucket groups, epochs and generations are natural numbers, so `omega` reads every comparison.

A read at epoch `e` sees, for each key, the latest event of that key stamped at or below `e`, where *latest* means
appended last (`lastWhere`). MVCC picks the event with the highest version instead; the two agree because every key's
stamps never decrease along its list, which `PlacementModel.Base` proves (`BaseInv.edgeSorted`, `BaseInv.memSorted`).

**Simplifications.** A version is its epoch: the sequence within an epoch is the order of the list, and only epochs are
compared by the properties proved here.
-/

namespace PlacementModel

/-! ## The latest element satisfying a predicate -/

/-- The element of `l` satisfying `q` that was appended last. -/
def lastWhere {α : Type} : List α → (α → Bool) → Option α
  | [], _ => none
  | x :: xs, q =>
    match lastWhere xs q with
    | some y => some y
    | none => if q x then some x else none

variable {α : Type}

theorem lastWhere_append_single (l : List α) (x : α) (q : α → Bool) :
    lastWhere (l ++ [x]) q = if q x then some x else lastWhere l q := by
  induction l with
  | nil => simp [lastWhere]
  | cons a l ih =>
    rw [List.cons_append]
    simp only [lastWhere]
    rw [ih]
    cases q x <;> simp

/-- Appending elements that fail `q` leaves the latest element satisfying `q` as it was. -/
theorem lastWhere_append_none (l X : List α) (q : α → Bool) (h : ∀ x ∈ X, q x = false) :
    lastWhere (l ++ X) q = lastWhere l q := by
  induction X generalizing l with
  | nil => rw [List.append_nil]
  | cons x xs ih =>
    rw [show l ++ x :: xs = (l ++ [x]) ++ xs by simp,
      ih (l ++ [x]) (fun y hy => h y (List.mem_cons_of_mem _ hy)), lastWhere_append_single,
      h x (List.mem_cons_self _ _)]
    rfl

theorem lastWhere_mem {l : List α} {q : α → Bool} {x : α} (h : lastWhere l q = some x) : x ∈ l ∧ q x = true := by
  induction l with
  | nil => simp [lastWhere] at h
  | cons a l ih =>
    simp only [lastWhere] at h
    cases hl : lastWhere l q with
    | some y =>
      rw [hl] at h; cases h
      exact ⟨List.mem_cons_of_mem _ (ih hl).1, (ih hl).2⟩
    | none =>
      rw [hl] at h
      cases hq : q a with
      | true => rw [hq] at h; cases h; exact ⟨List.mem_cons_self _ _, hq⟩
      | false => rw [hq] at h; cases h

theorem lastWhere_none {l : List α} {q : α → Bool} (h : lastWhere l q = none) : ∀ x ∈ l, q x = false := by
  induction l with
  | nil => intro x hx; cases hx
  | cons a l ih =>
    simp only [lastWhere] at h
    cases hl : lastWhere l q with
    | some y => rw [hl] at h; cases h
    | none =>
      rw [hl] at h
      intro x hx
      cases List.mem_cons.mp hx with
      | inl he =>
        subst he
        cases hq : q x with
        | true => rw [hq] at h; cases h
        | false => rfl
      | inr ht => exact ih hl x ht

/-- If no element satisfies `q`, none satisfies a stronger predicate. -/
theorem lastWhere_refine_none {l : List α} {q q' : α → Bool} (h : lastWhere l q = none)
    (himp : ∀ y, q' y = true → q y = true) : lastWhere l q' = none := by
  induction l with
  | nil => rfl
  | cons a l ih =>
    have hall := lastWhere_none h
    have hl : lastWhere l q = none := by
      simp only [lastWhere] at h
      cases hl : lastWhere l q with
      | some y => rw [hl] at h; cases h
      | none => rfl
    simp only [lastWhere, ih hl]
    cases hq : q' a with
    | true =>
      have := hall a (List.mem_cons_self _ _)
      rw [himp a hq] at this; cases this
    | false => rfl

/-- The latest element satisfying `q` is also the latest satisfying a stronger predicate it satisfies. -/
theorem lastWhere_refine {l : List α} {q q' : α → Bool} {x : α} (h : lastWhere l q = some x) (hx : q' x = true)
    (himp : ∀ y, q' y = true → q y = true) : lastWhere l q' = some x := by
  induction l with
  | nil => simp [lastWhere] at h
  | cons a l ih =>
    simp only [lastWhere] at h ⊢
    cases hl : lastWhere l q with
    | some y =>
      rw [hl] at h; cases h
      rw [ih hl]
    | none =>
      rw [hl] at h
      rw [lastWhere_refine_none hl himp]
      cases hq : q a with
      | true => rw [hq] at h; cases h; rw [hx]; rfl
      | false => rw [hq] at h; cases h

/-! ## Visibility: the flag of the latest matching event -/

/-- The flag of the latest element satisfying `q`, `false` when none does. -/
def visible (l : List α) (q : α → Bool) (f : α → Bool) : Bool :=
  match lastWhere l q with
  | some x => f x
  | none => false

theorem visible_append_single (l : List α) (x : α) (q f : α → Bool) :
    visible (l ++ [x]) q f = if q x then f x else visible l q f := by
  unfold visible
  rw [lastWhere_append_single]
  cases q x <;> rfl

theorem visible_append_none (l X : List α) (q f : α → Bool) (h : ∀ x ∈ X, q x = false) :
    visible (l ++ X) q f = visible l q f := by
  unfold visible; rw [lastWhere_append_none l X q h]

/-! ## Edge and member events -/

/-- An edge event: partition `part` inserts (`present`) or deletes the edge from subject `subj` to entity `obj` in a
commit stamped `stamp`. The entries an edge derives at its writer — forward, local reverse, object-predicates — are one
fact here, since they are written in one local commit. -/
structure EdgeEv where
  subj : Nat
  obj : Nat
  stamp : Nat
  part : Nat
  present : Bool
  deriving DecidableEq, Repr

/-- A member event of entity `obj`'s referrer set at its home: bucket group `grp` added or renewed (`present`) at
generation `gen`, or removed by reclamation, at epoch `epoch`. -/
structure MemEv where
  obj : Nat
  grp : Nat
  epoch : Nat
  gen : Nat
  present : Bool
  deriving DecidableEq, Repr

/-- Events of the edge from `s` to `o` stamped at or below `e`. -/
def edgeKey (s o e : Nat) (ev : EdgeEv) : Bool := decide (ev.subj = s ∧ ev.obj = o ∧ ev.stamp ≤ e)

/-- Every event of the edge from `s` to `o`. -/
def edgeKeyAll (s o : Nat) (ev : EdgeEv) : Bool := decide (ev.subj = s ∧ ev.obj = o)

/-- Events of member `g` of `R(o)` at or below epoch `e`. -/
def memKey (o g e : Nat) (ev : MemEv) : Bool := decide (ev.obj = o ∧ ev.grp = g ∧ ev.epoch ≤ e)

/-- Every event of member `g` of `R(o)`. -/
def memKeyAll (o g : Nat) (ev : MemEv) : Bool := decide (ev.obj = o ∧ ev.grp = g)

/-- The edge from `s` to `o` is visible at epoch `e`. -/
def edgeAt (edges : List EdgeEv) (s o e : Nat) : Bool := visible edges (edgeKey s o e) EdgeEv.present

/-- The edge from `s` to `o` is present in the latest state, as its writer sees it. -/
def edgeCur (edges : List EdgeEv) (s o : Nat) : Bool := visible edges (edgeKeyAll s o) EdgeEv.present

/-- Group `g` is a member of `R(o)` at epoch `e`. -/
def memAt (mems : List MemEv) (o g e : Nat) : Bool := visible mems (memKey o g e) MemEv.present

/-- The latest event of member `g` of `R(o)`, the state the home decides on. -/
def memCur (mems : List MemEv) (o g : Nat) : Option MemEv := lastWhere mems (memKeyAll o g)

/-- `g` is a member of `R(o)` at every epoch from `a` on. -/
def presentFrom (mems : List MemEv) (o g a : Nat) : Prop := ∀ e, a ≤ e → memAt mems o g e = true

/-- Some event added or renewed member `g` of `R(o)` at an epoch at or below `a`: a registration exists for it. -/
def HasReg (mems : List MemEv) (o g a : Nat) : Prop :=
  ∃ ev ∈ mems, ev.obj = o ∧ ev.grp = g ∧ ev.present = true ∧ ev.epoch ≤ a

/-- The highest generation member `g` of `R(o)` has had, removed members included. -/
def genMax : List MemEv → Nat → Nat → Nat
  | [], _, _ => 0
  | ev :: evs, o, g => if ev.obj = o ∧ ev.grp = g then max ev.gen (genMax evs o g) else genMax evs o g

/-! ## Lemmas about events -/

theorem genMax_append_single (l : List MemEv) (x : MemEv) (o g : Nat) :
    genMax (l ++ [x]) o g = if x.obj = o ∧ x.grp = g then max (genMax l o g) x.gen else genMax l o g := by
  induction l with
  | nil => simp only [List.nil_append, genMax]; split <;> omega
  | cons a l ih =>
    rw [List.cons_append]
    simp only [genMax]
    rw [ih]
    split <;> split <;> omega

theorem genMax_le {l : List MemEv} {ev : MemEv} (h : ev ∈ l) (o g : Nat) (ho : ev.obj = o) (hg : ev.grp = g) :
    ev.gen ≤ genMax l o g := by
  induction l with
  | nil => cases h
  | cons a l ih =>
    simp only [genMax]
    cases List.mem_cons.mp h with
    | inl he => subst he; rw [if_pos ⟨ho, hg⟩]; omega
    | inr ht => have := ih ht; split <;> omega

theorem genMax_append_mono (l : List MemEv) (x : MemEv) (o g : Nat) : genMax l o g ≤ genMax (l ++ [x]) o g := by
  rw [genMax_append_single]; split <;> omega

theorem memCur_mem {l : List MemEv} {o g : Nat} {x : MemEv} (h : memCur l o g = some x) :
    x ∈ l ∧ x.obj = o ∧ x.grp = g := by
  have := lastWhere_mem h
  simp [memKeyAll] at this
  exact ⟨this.1, this.2.1, this.2.2⟩

theorem memCur_append_single (l : List MemEv) (x : MemEv) (o g : Nat) :
    memCur (l ++ [x]) o g = if x.obj = o ∧ x.grp = g then some x else memCur l o g := by
  unfold memCur; rw [lastWhere_append_single]; simp [memKeyAll]

/-- Once the latest event of a member is at or below `e`, a read at `e` sees exactly it. -/
theorem memAt_of_cur {l : List MemEv} {o g e : Nat} {x : MemEv} (h : memCur l o g = some x) (he : x.epoch ≤ e) :
    memAt l o g e = x.present := by
  have hm := memCur_mem h
  unfold memAt visible
  rw [lastWhere_refine (q := memKeyAll o g) h (by simp [memKey, hm.2.1, hm.2.2, he])
    (fun y hy => by simp [memKey, memKeyAll] at hy ⊢; exact ⟨hy.1, hy.2.1⟩)]

theorem memAt_of_cur_none {l : List MemEv} {o g e : Nat} (h : memCur l o g = none) : memAt l o g e = false := by
  unfold memAt visible
  rw [lastWhere_refine_none (q := memKeyAll o g) h
    (fun y hy => by simp [memKey, memKeyAll] at hy ⊢; exact ⟨hy.1, hy.2.1⟩)]

theorem memAt_append_single (l : List MemEv) (x : MemEv) (o g e : Nat) :
    memAt (l ++ [x]) o g e = if x.obj = o ∧ x.grp = g ∧ x.epoch ≤ e then x.present else memAt l o g e := by
  unfold memAt; rw [visible_append_single]; simp [memKey]

/-- Appending a present member event never hides a member. -/
theorem memAt_append_present {l : List MemEv} {x : MemEv} (hx : x.present = true) {o g e : Nat}
    (h : memAt l o g e = true) : memAt (l ++ [x]) o g e = true := by
  rw [memAt_append_single]; split
  · exact hx
  · exact h

theorem presentFrom_append_present {l : List MemEv} {x : MemEv} (hx : x.present = true) {o g a : Nat}
    (h : presentFrom l o g a) : presentFrom (l ++ [x]) o g a :=
  fun e he => memAt_append_present hx (h e he)

/-- An event of another member leaves a member's visibility as it was. -/
theorem memAt_append_other {l : List MemEv} {x : MemEv} {o g : Nat} (h : ¬(x.obj = o ∧ x.grp = g)) (e : Nat) :
    memAt (l ++ [x]) o g e = memAt l o g e := by
  rw [memAt_append_single, if_neg (fun h' => h ⟨h'.1, h'.2.1⟩)]

/-- A present latest event at or below `a` keeps the member present from `a` on, until another event arrives. -/
theorem presentFrom_of_cur {l : List MemEv} {o g a : Nat} {x : MemEv} (h : memCur l o g = some x)
    (hp : x.present = true) (ha : x.epoch ≤ a) : presentFrom l o g a :=
  fun e he => by rw [memAt_of_cur h (by omega)]; exact hp

theorem edgeAt_append_single (l : List EdgeEv) (x : EdgeEv) (s o e : Nat) :
    edgeAt (l ++ [x]) s o e = if x.subj = s ∧ x.obj = o ∧ x.stamp ≤ e then x.present else edgeAt l s o e := by
  unfold edgeAt; rw [visible_append_single]; simp [edgeKey]

theorem edgeCur_append_single (l : List EdgeEv) (x : EdgeEv) (s o : Nat) :
    edgeCur (l ++ [x]) s o = if x.subj = s ∧ x.obj = o then x.present else edgeCur l s o := by
  unfold edgeCur; rw [visible_append_single]; simp [edgeKeyAll]

/-- An edge absent in the latest state, all of whose events are stamped at or below `r`, is absent at every epoch from
`r` on. -/
theorem edgeAt_false_of_cur {l : List EdgeEv} {s o r : Nat} (hc : edgeCur l s o = false)
    (hst : ∀ ev ∈ l, ev.subj = s → ev.obj = o → ev.stamp ≤ r) : ∀ e, r ≤ e → edgeAt l s o e = false := by
  intro e he
  unfold edgeCur visible at hc
  unfold edgeAt visible
  cases hl : lastWhere l (edgeKeyAll s o) with
  | none =>
    rw [lastWhere_refine_none (q := edgeKeyAll s o) hl
      (fun y hy => by simp [edgeKey, edgeKeyAll] at hy ⊢; exact ⟨hy.1, hy.2.1⟩)]
  | some x =>
    rw [hl] at hc
    have hm := lastWhere_mem hl
    simp [edgeKeyAll] at hm
    have hxr := hst x hm.1 hm.2.1 hm.2.2
    rw [lastWhere_refine (q := edgeKeyAll s o) hl (by simp [edgeKey, hm.2.1, hm.2.2]; omega)
      (fun y hy => by simp [edgeKey, edgeKeyAll] at hy ⊢; exact ⟨hy.1, hy.2.1⟩)]
    exact hc

end PlacementModel
