/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Catalog.Schema

/-!
# Constraints

`SCHEMA-APG.md` section 14: the state constraints of `C(K)` as violation queries over a finite APG, each with its
soundness, and the transition constraints as predicates on consecutive states.

A violation query returns the identifiers of the elements that break its constraint, and the constraint holds when it
returns nothing (section 14.1). Three generic queries carry the ten rows of section 14.2 that are not discharged by
typing:

- `pairViolations` returns each element in conflict with another; `injViolations` specialises it to a projection
  that must be injective where defined. `@key`, `@unique`, `@set`, role labels and `@no_overlap` are its instances.
- `violators` returns each candidate failing a test: every set-valued role non-empty, participation bounds, `@check`
  and `@limit`, per participant for a lower bound and per group its elements reach for an upper one.
- `cycleViolations` and `depthViolations` walk the references from a contained role to its container role:
  `@acyclic` and `max_depth`. A walk of as many references as there are vertices repeats a vertex (`pigeonhole`), so
  the bounded search finds every cycle.

Each row's `_nil_iff` theorem states that the query returns nothing exactly when the constraint holds. The constraints
take the labels of `E_g(T)`, `conc(T)` from `ApgModel.Catalog.Catalog`, as a list; the liveness of an element at the
read point is a predicate `live`, since a state holds its retracted elements (section 15.3). A required single-valued
field holds by typing (`FinApg.valid`) and has no query; section 14.2's graphs row is section 15's.
-/

namespace ApgModel.Catalog

open Named

variable {T : Typeside}

/-! ## Equality of value terms -/

/-- Decidable equality of value terms, given decidable equality of each primitive type's values. -/
def VTerm.decEq [DecidableEq T.P] [∀ p, DecidableEq (T.PV p)] : (a b : VTerm T) → Decidable (a = b)
  | .prim p a, .prim q b =>
    if h : p = q then
      match p, q, h with
      | _, _, rfl => if hab : a = b then isTrue (hab ▸ rfl) else isFalse (fun e => by cases e; exact hab rfl)
    else isFalse (fun e => by cases e; exact h rfl)
  | .elem e, .elem f => if h : e = f then isTrue (h ▸ rfl) else isFalse (fun w => by cases w; exact h rfl)
  | .record fs, .record gs =>
    match decFields fs gs with
    | isTrue h => isTrue (h ▸ rfl)
    | isFalse h => isFalse (fun e => by cases e; exact h rfl)
  | .inj n v, .inj m w =>
    if hn : n = m then
      match VTerm.decEq v w with
      | isTrue h => isTrue (by rw [hn, h])
      | isFalse h => isFalse (fun e => by cases e; exact h rfl)
    else isFalse (fun e => by cases e; exact hn rfl)
  | .list vs, .list ws =>
    match decAll vs ws with
    | isTrue h => isTrue (h ▸ rfl)
    | isFalse h => isFalse (fun e => by cases e; exact h rfl)
  | .set vs, .set ws =>
    match decAll vs ws with
    | isTrue h => isTrue (h ▸ rfl)
    | isFalse h => isFalse (fun e => by cases e; exact h rfl)
  | .prim _ _, .elem _ | .prim _ _, .record _ | .prim _ _, .inj _ _ | .prim _ _, .list _ | .prim _ _, .set _
  | .elem _, .prim _ _ | .elem _, .record _ | .elem _, .inj _ _ | .elem _, .list _ | .elem _, .set _
  | .record _, .prim _ _ | .record _, .elem _ | .record _, .inj _ _ | .record _, .list _ | .record _, .set _
  | .inj _ _, .prim _ _ | .inj _ _, .elem _ | .inj _ _, .record _ | .inj _ _, .list _ | .inj _ _, .set _
  | .list _, .prim _ _ | .list _, .elem _ | .list _, .record _ | .list _, .inj _ _ | .list _, .set _
  | .set _, .prim _ _ | .set _, .elem _ | .set _, .record _ | .set _, .inj _ _ | .set _, .list _ =>
    isFalse (fun e => by cases e)
where
  /-- Decidable equality of field lists. -/
  decFields : (fs gs : List (Name × VTerm T)) → Decidable (fs = gs)
    | [], [] => isTrue rfl
    | [], _ :: _ => isFalse (fun e => by cases e)
    | _ :: _, [] => isFalse (fun e => by cases e)
    | (n, v) :: fs, (m, w) :: gs =>
      if hn : n = m then
        match VTerm.decEq v w, decFields fs gs with
        | isTrue hv, isTrue hfs => isTrue (by rw [hn, hv, hfs])
        | isFalse hv, _ => isFalse (fun e => by cases e; exact hv rfl)
        | _, isFalse hfs => isFalse (fun e => by cases e; exact hfs rfl)
      else isFalse (fun e => by cases e; exact hn rfl)
  /-- Decidable equality of term lists. -/
  decAll : (vs ws : List (VTerm T)) → Decidable (vs = ws)
    | [], [] => isTrue rfl
    | [], _ :: _ => isFalse (fun e => by cases e)
    | _ :: _, [] => isFalse (fun e => by cases e)
    | v :: vs, w :: ws =>
      match VTerm.decEq v w, decAll vs ws with
      | isTrue hv, isTrue hvs => isTrue (by rw [hv, hvs])
      | isFalse hv, _ => isFalse (fun e => by cases e; exact hv rfl)
      | _, isFalse hvs => isFalse (fun e => by cases e; exact hvs rfl)

instance [DecidableEq T.P] [∀ p, DecidableEq (T.PV p)] : DecidableEq (VTerm T) := VTerm.decEq

/-! ## Generic violation queries -/

section Generic

variable {α β : Type}

/-- The elements in conflict with another: `x` is returned when an element of another identifier is `bad` with it. -/
def pairViolations (xs : List (Nat × α)) (bad : Nat × α → Nat × α → Bool) : List Nat :=
  (xs.filter fun x => xs.any fun y => x.1 != y.1 && bad x y).map Prod.fst

theorem pairViolations_nil_iff (xs : List (Nat × α)) (bad : Nat × α → Nat × α → Bool) :
    pairViolations xs bad = [] ↔ ∀ x ∈ xs, ∀ y ∈ xs, x.1 ≠ y.1 → bad x y = false := by
  simp only [pairViolations, List.map_eq_nil_iff, List.filter_eq_nil_iff, List.any_eq_true, Bool.and_eq_true,
    bne_iff_ne, ne_eq, not_exists, not_and, Bool.not_eq_true]

/-- The elements whose projection, where defined, another element's equals. -/
def injViolations [DecidableEq β] (xs : List (Nat × α)) (proj : Nat × α → Option β) : List Nat :=
  pairViolations xs fun x y =>
    match proj x, proj y with
    | some a, some b => decide (a = b)
    | _, _ => false

/-- An injectivity query returns nothing exactly when the projection is injective where defined. -/
theorem injViolations_nil_iff [DecidableEq β] (xs : List (Nat × α)) (proj : Nat × α → Option β) :
    injViolations xs proj = [] ↔ ∀ x ∈ xs, ∀ y ∈ xs, ∀ b, proj x = some b → proj y = some b → x.1 = y.1 := by
  rw [injViolations, pairViolations_nil_iff]
  constructor
  · intro h x hx y hy b hxb hyb
    refine Decidable.byContradiction fun hne => ?_
    have := h x hx y hy hne
    rw [hxb, hyb] at this
    simp at this
  · intro h x hx y hy hne
    cases hpx : proj x with
    | none => rfl
    | some a =>
      cases hpy : proj y with
      | none => rfl
      | some b =>
        simp only [decide_eq_false_iff_not]
        intro hab
        exact hne (h x hx y hy a hpx (hab ▸ hpy))

/-- The candidates failing a test. -/
def violators (xs : List (Nat × α)) (ok : Nat × α → Bool) : List Nat := (xs.filter fun x => !ok x).map Prod.fst

theorem violators_nil_iff (xs : List (Nat × α)) (ok : Nat × α → Bool) :
    violators xs ok = [] ↔ ∀ x ∈ xs, ok x = true := by
  simp only [violators, List.map_eq_nil_iff, List.filter_eq_nil_iff, Bool.not_eq_true', Bool.not_eq_false]

end Generic

/-! ## Walks and the pigeonhole -/

section Walks

/-- A walk of `k` references from `x` to `y` along the edges `E`. -/
inductive Walk (E : List (Nat × Nat)) : Nat → Nat → Nat → Prop where
  | nil (x : Nat) : Walk E x x 0
  | cons {x y z k : Nat} : (x, y) ∈ E → Walk E y z k → Walk E x z (k + 1)

/-- The ends of the walks of `k` references from `x`. -/
def reachIn (E : List (Nat × Nat)) : Nat → Nat → List Nat
  | 0, x => [x]
  | k + 1, x => (E.filter fun e => e.1 == x).flatMap fun e => reachIn E k e.2

theorem mem_reachIn (E : List (Nat × Nat)) : (k x y : Nat) → (y ∈ reachIn E k x ↔ Walk E x y k)
  | 0, x, y => by
    simp only [reachIn, List.mem_singleton]
    constructor
    · rintro rfl
      exact .nil y
    · intro h
      cases h
      rfl
  | k + 1, x, y => by
    simp only [reachIn, List.mem_flatMap, List.mem_filter, beq_iff_eq]
    constructor
    · rintro ⟨⟨a, b⟩, ⟨he, rfl⟩, hy⟩
      exact .cons he ((mem_reachIn E k b y).mp hy)
    · intro h
      cases h with
      | cons he hw => exact ⟨_, ⟨he, rfl⟩, (mem_reachIn E k _ y).mpr hw⟩

/-- The vertices the edges touch. -/
def verts (E : List (Nat × Nat)) : List Nat := E.map Prod.fst ++ E.map Prod.snd

variable {E : List (Nat × Nat)}

theorem Walk.append {x y z m n : Nat} (h : Walk E x y m) (h' : Walk E y z n) : Walk E x z (m + n) := by
  induction h with
  | nil => simpa using h'
  | cons he _ ih => rw [Nat.add_right_comm]; exact .cons he (ih h')

theorem Walk.repeat {x k : Nat} (h : Walk E x x k) : (j : Nat) → Walk E x x (k * j)
  | 0 => .nil x
  | j + 1 => by rw [Nat.mul_succ]; exact (h.repeat j).append h

theorem Walk.prefix {x y n : Nat} (h : Walk E x y n) : (m : Nat) → m ≤ n → ∃ z, Walk E x z m := by
  induction h with
  | nil x => intro m hm; exact ⟨x, by rw [Nat.le_zero.mp hm]; exact .nil x⟩
  | @cons x y z k he _ ih =>
    intro m hm
    cases m with
    | zero => exact ⟨x, .nil x⟩
    | succ m =>
      obtain ⟨w, hw⟩ := ih m (by omega)
      exact ⟨w, .cons he hw⟩

theorem Walk.start_mem {x y k : Nat} (h : Walk E x y k) (hk : 0 < k) : x ∈ verts E := by
  cases h with
  | nil => omega
  | cons he _ => exact List.mem_append_left _ (List.mem_map.mpr ⟨_, he, rfl⟩)

/-- A list of vertices joined by edges. -/
def IsWalk (E : List (Nat × Nat)) : List Nat → Prop
  | a :: b :: r => (a, b) ∈ E ∧ IsWalk E (b :: r)
  | _ => True

theorem walk_of_mem : (b : Nat) → (r : List Nat) → IsWalk E (b :: r) → ∀ z ∈ b :: r, ∃ k, Walk E b z k
  | b, [], _, z, hz => by
    rw [List.mem_singleton] at hz
    subst hz
    exact ⟨0, .nil _⟩
  | b, c :: r, ⟨hbc, hw⟩, z, hz => by
    rcases List.mem_cons.mp hz with rfl | hz
    · exact ⟨0, .nil z⟩
    · obtain ⟨k, hk⟩ := walk_of_mem c r hw z hz
      exact ⟨k + 1, .cons hbc hk⟩

/-- A walk through a repeated vertex holds a cycle. -/
theorem cycle_of_not_nodup : (vs : List Nat) → IsWalk E vs → ¬ vs.Nodup → ∃ a k, 0 < k ∧ Walk E a a k
  | [], _, hn => absurd List.nodup_nil hn
  | a :: rest, hw, hn => by
    rw [List.nodup_cons] at hn
    by_cases ha : a ∈ rest
    · match rest, hw, ha with
      | b :: r, ⟨hab, hw'⟩, ha =>
        obtain ⟨k, hk⟩ := walk_of_mem b r hw' a ha
        exact ⟨a, k + 1, by omega, .cons hab hk⟩
    · have hrest : IsWalk E rest := by
        match rest, hw with
        | [], _ => trivial
        | _ :: _, ⟨_, h⟩ => exact h
      exact cycle_of_not_nodup rest hrest (fun h => hn ⟨ha, h⟩)

theorem walk_verts {x y k : Nat} (h : Walk E x y k) :
    ∃ vs : List Nat, IsWalk E (x :: vs) ∧ vs.length = k ∧ ∀ v ∈ vs, v ∈ verts E := by
  induction h with
  | nil => exact ⟨[], trivial, rfl, fun _ h => nomatch h⟩
  | @cons x y z k he _ ih =>
    obtain ⟨vs, hw, hl, hv⟩ := ih
    refine ⟨y :: vs, ⟨he, hw⟩, by simp [hl], fun v hv' => ?_⟩
    rcases List.mem_cons.mp hv' with rfl | hv'
    · exact List.mem_append_right _ (List.mem_map.mpr ⟨_, he, rfl⟩)
    · exact hv v hv'

/-- The pigeonhole: a list without repetitions inside `V` is no longer than `V`. -/
theorem pigeonhole : (l V : List Nat) → l.Nodup → (∀ a ∈ l, a ∈ V) → l.length ≤ V.length
  | [], _, _, _ => Nat.zero_le _
  | a :: l, V, hn, hs => by
    rw [List.nodup_cons] at hn
    have ha : a ∈ V := hs a (List.mem_cons_self a l)
    have := pigeonhole l (V.erase a) hn.2 fun b hb =>
      (List.mem_erase_of_ne (fun h => hn.1 (by rw [← h]; exact hb))).mpr (hs b (List.mem_cons_of_mem a hb))
    rw [List.length_erase_of_mem ha] at this
    have : 0 < V.length := List.length_pos_of_mem ha
    simp only [List.length_cons]
    omega

/-- The vertices on a cycle, found among the walks of as many references as there are vertices. -/
def cycleViolations (E : List (Nat × Nat)) : List Nat :=
  (verts E).filter fun x => !(reachIn E (verts E).length x).isEmpty

theorem cycleViolations_nil_iff : cycleViolations E = [] ↔ ∀ x k, 0 < k → ¬ Walk E x x k := by
  simp only [cycleViolations, List.filter_eq_nil_iff, Bool.not_eq_true', List.isEmpty_eq_false_iff_exists_mem,
    not_exists]
  constructor
  · intro h x k hk hw
    obtain ⟨y, hy⟩ := (hw.repeat (verts E).length).prefix (verts E).length
      (Nat.le_mul_of_pos_left _ hk)
    exact h x (hw.start_mem hk) y ((mem_reachIn E _ x y).mpr hy)
  · intro h x hx y hy
    obtain ⟨vs, hw, hl, hv⟩ := walk_verts ((mem_reachIn E _ x y).mp hy)
    have hnd : ¬ (x :: vs).Nodup := fun hnd => by
      have := pigeonhole (x :: vs) (verts E) hnd fun v hv' => by
        rcases List.mem_cons.mp hv' with rfl | hv'
        · exact hx
        · exact hv v hv'
      simp only [List.length_cons, hl] at this
      omega
    obtain ⟨a, k, hk, hc⟩ := cycle_of_not_nodup (x :: vs) hw hnd
    exact h a k hk hc

/-- The vertices starting a chain of more than `d` references. -/
def depthViolations (E : List (Nat × Nat)) (d : Nat) : List Nat :=
  (verts E).filter fun x => !(reachIn E (d + 1) x).isEmpty

theorem depthViolations_nil_iff (d : Nat) : depthViolations E d = [] ↔ ∀ x y, ¬ Walk E x y (d + 1) := by
  simp only [depthViolations, List.filter_eq_nil_iff, Bool.not_eq_true', List.isEmpty_eq_false_iff_exists_mem,
    not_exists]
  constructor
  · intro h x y hw
    exact h x (hw.start_mem (Nat.succ_pos d)) y ((mem_reachIn E _ x y).mpr hw)
  · intro h x _ y hy
    exact h x y ((mem_reachIn E _ x y).mp hy)

end Walks

/-! ## Section 14.2: the state constraints -/

variable {L : Type} [DecidableEq L] {S : NSchema T.P L}

/-- The references a value term's field holds; none when the field is absent. -/
def fieldRefs (v : VTerm T) (n : Name) : List Nat :=
  match v.field? n with
  | some w => w.refs
  | none => []

/-- `E(ls)`: the elements carrying one of the labels `ls`. -/
def ofLabels (G : FinApg S) (ls : List L) : List (Nat × L × VTerm T) := G.elems.filter fun x => decide (x.2.1 ∈ ls)

theorem mem_ofLabels {G : FinApg S} {ls : List L} {x : Nat × L × VTerm T} :
    x ∈ ofLabels G ls ↔ x ∈ G.elems ∧ x.2.1 ∈ ls := by
  simp [ofLabels]

/-- Two lists hold the same members. -/
def sameSet (l m : List Nat) : Bool := l.all (· ∈ m) && m.all (· ∈ l)

theorem sameSet_iff (l m : List Nat) : sameSet l m = true ↔ ∀ p, p ∈ l ↔ p ∈ m := by
  simp only [sameSet, Bool.and_eq_true, List.all_eq_true, decide_eq_true_eq]
  constructor
  · rintro ⟨h1, h2⟩ p
    exact ⟨h1 p, h2 p⟩
  · intro h
    exact ⟨fun p => (h p).mp, fun p => (h p).mpr⟩

section Rows

variable [DecidableEq T.P] [∀ p, DecidableEq (T.PV p)] (G : FinApg S)

/-- A key's fields of a value term. -/
def keyOf (ks : List Name) (v : VTerm T) : List (Option (VTerm T)) := ks.map v.field?

/-- `@key(k₁, …)` on a tree whose concrete types are `ls`: the elements sharing their key with another, retracted
elements included. -/
def keyViolations (ls : List L) (ks : List Name) : List Nat :=
  injViolations (ofLabels G ls) fun x => some (keyOf ks x.2.2)

theorem keyViolations_nil_iff (ls : List L) (ks : List Name) :
    keyViolations G ls ks = [] ↔
      ∀ x ∈ ofLabels G ls, ∀ y ∈ ofLabels G ls, keyOf ks x.2.2 = keyOf ks y.2.2 → x.1 = y.1 := by
  rw [keyViolations, injViolations_nil_iff]
  constructor
  · intro h x hx y hy hk
    exact h x hx y hy _ rfl (by rw [hk])
  · intro h x hx y hy b hxb hyb
    exact h x hx y hy (Option.some.inj (hxb.trans hyb.symm))

/-- An attribute's value where present: an optional attribute's absence is the `none` injection. -/
def presentValue (optional : Bool) (v : VTerm T) : Option (VTerm T) :=
  if optional then
    match v with
    | .inj "some" w => some w
    | _ => none
  else some v

/-- An attribute of a value term where present. -/
def attrOf (a : Name) (optional : Bool) (v : VTerm T) : Option (VTerm T) := (v.field? a).bind (presentValue optional)

/-- `@unique a`: the elements sharing their present value of `a` with another, retracted elements included. -/
def uniqueViolations (ls : List L) (a : Name) (optional : Bool) : List Nat :=
  injViolations (ofLabels G ls) fun x => attrOf a optional x.2.2

theorem uniqueViolations_nil_iff (ls : List L) (a : Name) (optional : Bool) :
    uniqueViolations G ls a optional = [] ↔
      ∀ x ∈ ofLabels G ls, ∀ y ∈ ofLabels G ls, ∀ w,
        attrOf a optional x.2.2 = some w → attrOf a optional y.2.2 = some w → x.1 = y.1 :=
  injViolations_nil_iff _ _

/-- A role label's element's instance and participant. -/
def roleLabelKey (v : VTerm T) : Option (VTerm T) × Option (VTerm T) := (v.field? "instance", v.field? "participant")

/-- A role label `R_r`: the elements naming the same instance and participant as another. -/
def roleLabelViolations (Rr : L) : List Nat :=
  injViolations (ofLabels G [Rr]) fun x => some (roleLabelKey x.2.2)

theorem roleLabelViolations_nil_iff (Rr : L) :
    roleLabelViolations G Rr = [] ↔
      ∀ x ∈ ofLabels G [Rr], ∀ y ∈ ofLabels G [Rr], roleLabelKey x.2.2 = roleLabelKey y.2.2 → x.1 = y.1 := by
  rw [roleLabelViolations, injViolations_nil_iff]
  constructor
  · intro h x hx y hy hk
    exact h x hx y hy _ rfl (by rw [hk])
  · intro h x hx y hy b hxb hyb
    exact h x hx y hy (Option.some.inj (hxb.trans hyb.symm))

/-- The participants an instance `e` of a relation names through its set-valued role's label `R_r`. -/
def setParticipants (Rr : L) (e : Nat) : List Nat :=
  ((ofLabels G [Rr]).filter fun y => decide (y.2.2.field? "instance" = some (.elem e))).flatMap fun y =>
    fieldRefs y.2.2 "participant"

/-- The single-valued roles of two elements agree: as an unordered pair for a `@symmetric` relation. -/
def SameRoles (singles : List Name) (symmetric : Bool) (v w : VTerm T) : Prop :=
  if symmetric then ∀ p, p ∈ singles.flatMap (fieldRefs v) ↔ p ∈ singles.flatMap (fieldRefs w)
  else singles.map (fieldRefs v) = singles.map (fieldRefs w)

/-- `SameRoles` decided. -/
def sameRoles (singles : List Name) (symmetric : Bool) (v w : VTerm T) : Bool :=
  if symmetric then sameSet (singles.flatMap (fieldRefs v)) (singles.flatMap (fieldRefs w))
  else decide (singles.map (fieldRefs v) = singles.map (fieldRefs w))

omit [DecidableEq T.P] [∀ p, DecidableEq (T.PV p)] in
theorem sameRoles_iff (singles : List Name) (symmetric : Bool) (v w : VTerm T) :
    sameRoles singles symmetric v w = true ↔ SameRoles singles symmetric v w := by
  unfold sameRoles SameRoles
  cases symmetric
  · simp
  · simp only [if_true]
    exact sameSet_iff _ _

/-- `@set` on the relation `R`, its single-valued roles `singles` and the labels `R_r` of its set-valued roles: the
live elements agreeing with another on every role, a set-valued role's participants compared as sets. -/
def setViolations (live : Nat → Bool) (R : L) (singles : List Name) (symmetric : Bool) (setRoles : List L) :
    List Nat :=
  pairViolations ((ofLabels G [R]).filter fun x => live x.1) fun x y =>
    sameRoles singles symmetric x.2.2 y.2.2 &&
      setRoles.all fun Rr => sameSet (setParticipants G Rr x.1) (setParticipants G Rr y.1)

theorem setViolations_nil_iff (live : Nat → Bool) (R : L) (singles : List Name) (symmetric : Bool)
    (setRoles : List L) :
    setViolations G live R singles symmetric setRoles = [] ↔
      ∀ x ∈ ofLabels G [R], ∀ y ∈ ofLabels G [R], live x.1 = true → live y.1 = true → x.1 ≠ y.1 →
        ¬ (SameRoles singles symmetric x.2.2 y.2.2 ∧
          ∀ Rr ∈ setRoles, ∀ p, p ∈ setParticipants G Rr x.1 ↔ p ∈ setParticipants G Rr y.1) := by
  rw [setViolations, pairViolations_nil_iff]
  simp only [List.mem_filter, Bool.and_eq_false_iff, List.all_eq_false, Bool.not_eq_true]
  constructor
  · intro h x hx y hy hlx hly hne ⟨hr, hs⟩
    rcases h x ⟨hx, hlx⟩ y ⟨hy, hly⟩ hne with h1 | ⟨Rr, hRr, h2⟩
    · rw [← Bool.not_eq_true, sameRoles_iff] at h1
      exact h1 hr
    · rw [← Bool.not_eq_true, sameSet_iff] at h2
      exact h2 (hs Rr hRr)
  · intro h x ⟨hx, hlx⟩ y ⟨hy, hly⟩ hne
    by_cases hr : sameRoles singles symmetric x.2.2 y.2.2 = true
    · right
      refine Decidable.byContradiction fun hall => ?_
      simp only [not_exists, not_and, Bool.not_eq_false] at hall
      exact h x hx y hy hlx hly hne
        ⟨(sameRoles_iff _ _ _ _).mp hr, fun Rr hRr => (sameSet_iff _ _).mp (hall Rr hRr)⟩
    · left
      simpa using hr

/-- Every set-valued role non-empty: the elements of `R` its role label `R_r` names no participant of. -/
def nonEmptyViolations (R Rr : L) : List Nat :=
  violators (ofLabels G [R]) fun x => (ofLabels G [Rr]).any fun y => decide (y.2.2.field? "instance" = some (.elem x.1))

theorem nonEmptyViolations_nil_iff (R Rr : L) :
    nonEmptyViolations G R Rr = [] ↔
      ∀ x ∈ ofLabels G [R], ∃ y ∈ ofLabels G [Rr], y.2.2.field? "instance" = some (.elem x.1) := by
  rw [nonEmptyViolations, violators_nil_iff]
  simp only [List.any_eq_true, decide_eq_true_eq]

/-- The number of live elements of the labels `src` reaching `p` through their field `r`. -/
def reaching (live : Nat → Bool) (src : List L) (r : Name) (p : Nat) : Nat :=
  ((ofLabels G src).filter fun y => live y.1 && p ∈ fieldRefs y.2.2 r).length

/-- Participation `?`, `+`, `{m, n}` of the role `r` of `src`, its participant types' concrete types `ls`: the live
participants reached by fewer than `lo` or more than `hi` live elements. A set-valued role's count is its role
label's, `src` being `[R_r]` and `r` its participant field. -/
def participationViolations (live : Nat → Bool) (src : List L) (r : Name) (ls : List L) (lo : Nat) (hi : Option Nat) :
    List Nat :=
  violators ((ofLabels G ls).filter fun x => live x.1) fun x =>
    decide (lo ≤ reaching G live src r x.1) && hi.all (reaching G live src r x.1 ≤ ·)

omit [DecidableEq T.P] [∀ p, DecidableEq (T.PV p)] in
theorem participationViolations_nil_iff (live : Nat → Bool) (src : List L) (r : Name) (ls : List L) (lo : Nat)
    (hi : Option Nat) :
    participationViolations G live src r ls lo hi = [] ↔
      ∀ x ∈ ofLabels G ls, live x.1 = true →
        lo ≤ reaching G live src r x.1 ∧ ∀ h ∈ hi, reaching G live src r x.1 ≤ h := by
  rw [participationViolations, violators_nil_iff]
  simp only [List.mem_filter]
  cases hi with
  | none => simp
  | some b => simp [Option.all]

/-- `@check`, `@range`, `@pattern`, `@values` on the types `ls`: the elements whose value fails the predicate. -/
def checkViolations (ls : List L) (c : VTerm T → Bool) : List Nat := violators (ofLabels G ls) fun x => c x.2.2

omit [DecidableEq T.P] [∀ p, DecidableEq (T.PV p)] in
theorem checkViolations_nil_iff (ls : List L) (c : VTerm T → Bool) :
    checkViolations G ls c = [] ↔ ∀ x ∈ ofLabels G ls, c x.2.2 = true :=
  violators_nil_iff _ _

/-- The references from the contained role to the container role over the elements of `R` selected by `sel`, the live
elements or, under `@acyclic(open)`, the open ones. -/
def containment (sel : Nat → Bool) (R : L) (contained container : Name) : List (Nat × Nat) :=
  ((ofLabels G [R]).filter fun x => sel x.1).flatMap fun x =>
    (fieldRefs x.2.2 contained).flatMap fun a => (fieldRefs x.2.2 container).map fun b => (a, b)

/-- `@acyclic`: the elements on a cycle of containment. -/
def acyclicViolations (sel : Nat → Bool) (R : L) (contained container : Name) : List Nat :=
  cycleViolations (containment G sel R contained container)

omit [DecidableEq T.P] [∀ p, DecidableEq (T.PV p)] in
theorem acyclicViolations_nil_iff (sel : Nat → Bool) (R : L) (contained container : Name) :
    acyclicViolations G sel R contained container = [] ↔
      ∀ x k, 0 < k → ¬ Walk (containment G sel R contained container) x x k :=
  cycleViolations_nil_iff

/-- `max_depth d`: the elements starting a chain of more than `d` containment references. -/
def maxDepthViolations (sel : Nat → Bool) (R : L) (contained container : Name) (d : Nat) : List Nat :=
  depthViolations (containment G sel R contained container) d

omit [DecidableEq T.P] [∀ p, DecidableEq (T.PV p)] in
theorem maxDepthViolations_nil_iff (sel : Nat → Bool) (R : L) (contained container : Name) (d : Nat) :
    maxDepthViolations G sel R contained container d = [] ↔
      ∀ x y, ¬ Walk (containment G sel R contained container) x y (d + 1) :=
  depthViolations_nil_iff d

/-- `@no_overlap(per: P)` on `R`, `interval` reading an element's valid interval: the live elements agreeing with
another on the roles `P` whose interval overlaps the other's. -/
def noOverlapViolations {I : Type} (live : Nat → Bool) (R : L) (per : List Name) (interval : VTerm T → Option I)
    (overlaps : I → I → Bool) : List Nat :=
  pairViolations ((ofLabels G [R]).filter fun x => live x.1) fun x y =>
    decide (per.map (fieldRefs x.2.2) = per.map (fieldRefs y.2.2)) &&
      match interval x.2.2, interval y.2.2 with
      | some i, some j => overlaps i j
      | _, _ => false

omit [DecidableEq T.P] [∀ p, DecidableEq (T.PV p)] in
theorem noOverlapViolations_nil_iff {I : Type} (live : Nat → Bool) (R : L) (per : List Name)
    (interval : VTerm T → Option I) (overlaps : I → I → Bool) :
    noOverlapViolations G live R per interval overlaps = [] ↔
      ∀ x ∈ ofLabels G [R], ∀ y ∈ ofLabels G [R], live x.1 = true → live y.1 = true → x.1 ≠ y.1 →
        per.map (fieldRefs x.2.2) = per.map (fieldRefs y.2.2) →
          ∀ i j, interval x.2.2 = some i → interval y.2.2 = some j → overlaps i j = false := by
  rw [noOverlapViolations, pairViolations_nil_iff]
  simp only [List.mem_filter, Bool.and_eq_false_iff, decide_eq_false_iff_not]
  constructor
  · intro h x hx y hy hlx hly hne hper i j hi hj
    rcases h x ⟨hx, hlx⟩ y ⟨hy, hly⟩ hne with h1 | h2
    · exact absurd hper h1
    · rw [hi, hj] at h2
      exact h2
  · intro h x ⟨hx, hlx⟩ y ⟨hy, hly⟩ hne
    by_cases hper : per.map (fieldRefs x.2.2) = per.map (fieldRefs y.2.2)
    · right
      cases hi : interval x.2.2 with
      | none => rfl
      | some i =>
        cases hj : interval y.2.2 with
        | none => rfl
        | some j => exact h x hx y hy hlx hly hne hper i j hi hj
    · left
      exact hper

/-- The live elements of the labels `rs` satisfying `c` that reach the participant `p` through the role `per`. -/
def groupOf (live : Nat → Bool) (rs : List L) (per : Name) (c : VTerm T → Bool) (p : Nat) :
    List (Nat × L × VTerm T) :=
  (ofLabels G rs).filter fun y => live y.1 && p ∈ fieldRefs y.2.2 per && c y.2.2

/-- `@limit(agg per r where c) op v` with one `per` role and no `type`, the form a lower bound takes (`UGS0529`), on
the relation whose concrete types are `rs`: the live participants of the types `ls` selected by the conjuncts of `c`
that read only their types (`select`), whose group's aggregate fails the comparison `holds`, a group no element
reaches included. -/
def limitViolations {A : Type} (live : Nat → Bool) (rs : List L) (per : Name) (ls : List L) (select : VTerm T → Bool)
    (c : VTerm T → Bool) (agg : List (Nat × L × VTerm T) → A) (holds : A → Bool) : List Nat :=
  violators ((ofLabels G ls).filter fun x => live x.1 && select x.2.2) fun x =>
    holds (agg (groupOf G live rs per c x.1))

omit [DecidableEq T.P] [∀ p, DecidableEq (T.PV p)] in
theorem limitViolations_nil_iff {A : Type} (live : Nat → Bool) (rs : List L) (per : Name) (ls : List L)
    (select : VTerm T → Bool) (c : VTerm T → Bool) (agg : List (Nat × L × VTerm T) → A) (holds : A → Bool) :
    limitViolations G live rs per ls select c agg holds = [] ↔
      ∀ x ∈ ofLabels G ls, live x.1 = true → select x.2.2 = true →
        holds (agg (groupOf G live rs per c x.1)) = true := by
  rw [limitViolations, violators_nil_iff]
  simp only [List.mem_filter, Bool.and_eq_true]
  exact ⟨fun h x hx hl hs => h x ⟨hx, hl, hs⟩, fun h x ⟨hx, hl, hs⟩ => h x hx hl hs⟩

/-- An element's group under `per (roles [, type])`: its participants in the roles and, under `type`, its concrete
type. -/
def groupKey (per : List Name) (byType : Bool) (x : Nat × L × VTerm T) : List (List Nat) × Option L :=
  (per.map (fieldRefs x.2.2), if byType then some x.2.1 else none)

/-- The live elements of the labels `rs` satisfying `c` in the group `k`. -/
def groupMembers (live : Nat → Bool) (rs : List L) (per : List Name) (byType : Bool) (c : VTerm T → Bool)
    (k : List (List Nat) × Option L) : List (Nat × L × VTerm T) :=
  (ofLabels G rs).filter fun y => live y.1 && c y.2.2 && decide (groupKey per byType y = k)

/-- `@limit(agg per (roles [, type]) where c) op v` on the relation whose concrete types are `rs`, its groups the ones
its elements reach, which is every group an upper bound can fail: the live elements satisfying `c` whose group, selected
by the conjuncts of `c` over its participants' types (`select`), has an aggregate failing the comparison `holds`. -/
def limitGroupViolations {A : Type} (live : Nat → Bool) (rs : List L) (per : List Name) (byType : Bool)
    (select : Nat × L × VTerm T → Bool) (c : VTerm T → Bool) (agg : List (Nat × L × VTerm T) → A)
    (holds : A → Bool) : List Nat :=
  violators ((ofLabels G rs).filter fun x => live x.1 && c x.2.2 && select x) fun x =>
    holds (agg (groupMembers G live rs per byType c (groupKey per byType x)))

omit [DecidableEq T.P] [∀ p, DecidableEq (T.PV p)] in
theorem limitGroupViolations_nil_iff {A : Type} (live : Nat → Bool) (rs : List L) (per : List Name) (byType : Bool)
    (select : Nat × L × VTerm T → Bool) (c : VTerm T → Bool) (agg : List (Nat × L × VTerm T) → A)
    (holds : A → Bool) :
    limitGroupViolations G live rs per byType select c agg holds = [] ↔
      ∀ x ∈ ofLabels G rs, live x.1 = true → c x.2.2 = true → select x = true →
        holds (agg (groupMembers G live rs per byType c (groupKey per byType x))) = true := by
  rw [limitGroupViolations, violators_nil_iff]
  simp only [List.mem_filter, Bool.and_eq_true]
  exact ⟨fun h x hx hl hc hs => h x ⟨hx, ⟨hl, hc⟩, hs⟩, fun h x ⟨hx, ⟨hl, hc⟩, hs⟩ => h x hx hl hc hs⟩

end Rows

/-! ## Section 14.3: the transition constraints -/

/-- A state at a read point: a finite APG with each element's liveness and whether its valid interval is open. -/
structure State (S : NSchema T.P L) where
  /-- The elements and their values. -/
  G : FinApg S
  /-- Whether an element is live, not retracted. -/
  live : Nat → Bool
  /-- Whether an element's valid interval is open. -/
  isOpen : Nat → Bool

namespace State

variable (s s' : State S)

/-- The element `e` is in the state. -/
def Has (e : Nat) : Prop := (s.G.lam e).isSome

/-- The element `e`'s value term. -/
def value (e : Nat) : Option (VTerm T) := (s.G.entry e).map Prod.snd

/-- Type fixed for life (`UGS1004`): an element of `s` in `s'` carries the label it carries in `s`. -/
def LabelFixed : Prop := ∀ e l l', s.G.lam e = some l → s'.G.lam e = some l' → l = l'

/-- Key immutable and `@immutable` (`UGS1007`, `UGS1012`): the fields `fs` of each element of the types `ls` are
equal before and after. -/
def FieldsFixed (ls : List L) (fs : List Name) : Prop :=
  ∀ e l v v', s.G.entry e = some (l, v) → l ∈ ls → s'.value e = some v' → ∀ f ∈ fs, v.field? f = v'.field? f

/-- `@append_only` (`UGS1013`): each element of the types `ls` stays with an equal value. -/
def AppendOnly (ls : List L) : Prop := ∀ e l v, s.G.entry e = some (l, v) → l ∈ ls → s'.G.entry e = some (l, v)

/-- `@bitemporal`: each element of the types `ls` stays, a change of value being a revision; it leaves the live slice
only by retraction. -/
def Bitemporal (ls : List L) : Prop := ∀ e l, s.G.lam e = some l → l ∈ ls → s'.Has e

/-- A role's action when its participant is deleted. -/
abbrev OnDelete := DeleteAction

/-- The `none` of an optional field. -/
def absent : VTerm T := .inj "none" (.record [])

/-- `@on_delete` of the role `r` of `R`: when `e` is deleted, each element of `R` referencing it through `r` is deleted
under `cascade`, refuses the commit under `restrict`, and has `r` absent under `orphan`; `s'` stays referentially
closed (proposition 6.6). -/
def OnDeleteHolds [DecidableEq T.P] (R : L) (r : Name) (a : OnDelete) : Prop :=
  s'.G.valid = true ∧
    ∀ e, s.Has e → ¬ s'.Has e → ∀ x v, s.G.entry x = some (R, v) → e ∈ fieldRefs v r →
      match a with
      | .cascade => ¬ s'.Has x
      | .restrict => False
      | .orphan => ∃ v', s'.value x = some v' ∧ v'.field? r = some absent

/-- A role's action when its participant is retracted. -/
abbrev OnRetract := RetractAction

/-- `@on_retract` of the role `r` of the `@bitemporal` relation `R`: when `e` is retracted, each live element of `R`
referencing it through `r` is retracted under `cascade`; under `restrict` none is live; under `restrict open` none is
open, and each live one is retracted with `e`. -/
def OnRetractHolds (R : L) (r : Name) (a : OnRetract) : Prop :=
  ∀ e, s.live e = true → s'.Has e → s'.live e = false → ∀ x v, s.G.entry x = some (R, v) → e ∈ fieldRefs v r →
    match a with
    | .cascade => s.live x = true → s'.live x = false
    | .restrict => s.live x = false
    | .restrictOpen => s.isOpen x = false ∧ (s.live x = true → s'.live x = false)

/-- `@retracts(r)` on `R`: an element of `R` inserted into `s'` retracts its participant in `r`. -/
def RetractsHolds (R : L) (r : Name) : Prop :=
  ∀ x v, ¬ s.Has x → s'.G.entry x = some (R, v) → ∀ p ∈ fieldRefs v r, s'.live p = false

/-- `@ends(r)` on `R`: an element of `R` inserted into `s'` closes the valid interval of its participant in `r`. -/
def EndsHolds (R : L) (r : Name) : Prop :=
  ∀ x v, ¬ s.Has x → s'.G.entry x = some (R, v) → ∀ p ∈ fieldRefs v r, s'.isOpen p = false

/-- Live participants (`UGS1019`): an element inserted into `s'` references only elements in `s'` and live. -/
def LiveParticipants : Prop :=
  ∀ x l v, ¬ s.Has x → s'.G.entry x = some (l, v) → ∀ p ∈ v.refs, s'.Has p ∧ s'.live p = true

end State

end ApgModel.Catalog
