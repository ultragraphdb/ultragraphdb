# Algebraic property graphs

* Status: first draft, 2026-11-08. The formal semantics of UGS, in three parts.

This file defines the algebraic property graph (APG) model of Shinavier, Wisnesky and Meyers, the model the formal
semantics of UGS is built on: a schema of labels whose types are records and variants over primitive types and labels,
a graph of elements each carrying one value of its label's type, the homomorphisms between such graphs, and the
schema mappings that carry a graph from one schema to another. It states the definitions in the named form CQL
implements, proves that form equivalent to the paper's binary one, and adds the results UltraGraphDB relies on.

Part 1, sections 1 to 11, is the model. Part 2, section 12, extends it with collections, for UGS's list and set
attributes and its set-valued roles. Part 3, sections 13 to 19, maps a UGS catalog onto the model, its labels, types,
constraints, names, graphs and history, and defines the RDF view and the GQL view as functions of one APG: the mapping
contract's definition, and the test oracle the engine and the query cores are checked against.

The model is the foundation of four layers (section 10): the APG model itself; the GQL view, described by a closed
graph type with PG-Schema's semantics; keys and other constraints with PG-Keys' semantics; and the RDF view with
RDF 1.2 reifiers and SHACL shapes.

## Contents

1. Conventions
2. Primitives
3. Types
4. Schemas
5. Values
6. Algebraic property graphs
7. Homomorphisms
8. Equivalent formulations
9. Schema mappings and Δ
10. The model and the views
11. UGS constructs in the model
12. Collections (part 2)
13. The schema of a catalog (part 3)
14. Constraints
15. Identity, graphs and time
16. Selections, key lookups, extended terms and coproducts
17. The RDF view
18. The GQL view
19. The test oracle
20. The Lean model
21. Decisions
22. Open questions
23. Sources

## 1. Conventions

| Notation | Meaning |
|---|---|
| Paper §n | Section n of *Algebraic Property Graphs*, arXiv:1909.04881, version 3 (20 July 2022) |
| CQL | The `catdata.apg` package of CQL, read for its representation of the paper's definitions |
| `A ⇀ B` | A finite partial function from `A` to `B`, with domain `dom φ` |
| `Set^L` | Families of sets indexed by a set `L`: `X = (X(l))_{l ∈ L}`; a morphism `h : X → Y` is a family of functions `h_l : X(l) → Y(l)` |
| `A + B`, `A × B` | Disjoint union and cartesian product of sets |
| Paper, here | A proposition marked *paper* is stated in the paper; one marked *here* is this file's, with its proof sketched |

Definitions are stated so that each transcribes into a Lean 4 inductive type or structure.

## 2. Primitives

**Definition 2.1 (typeside).** A typeside is `T = (P, PV, F, ⟦·⟧)`:

- `P`, a set of primitive types;
- `PV`, a set `PV(p)` of primitive values for each `p ∈ P`;
- `F`, a set of function symbols, each with an arity `f : p₁ … pₖ → p` over `P`;
- `⟦f⟧ : PV(p₁) × … × PV(pₖ) → PV(p)`, a total function for each `f`.

`P` and `PV` are the paper's parameters (paper §3). `F` holds the paper's user-defined functions (paper §6.1), narrowed
to primitive argument and result types where the paper admits any label-free type, so a function never depends on a
schema. The paper's equations between such functions are left out here; part 3 places UGS's value checks. Every
definition below is relative to one typeside.

## 3. Types

**Definition 3.1 (field names).** `N` is a countable set of field names.

**Definition 3.2 (types).** For a set `L` of labels, `Ty(L)` is the least set closed under:

```
prim p     for p ∈ P               a primitive type
lbl l      for l ∈ L               a reference to an element labelled l
rec φ      for φ : N ⇀ Ty(L)       a record: one value for every field of dom φ
var φ      for φ : N ⇀ Ty(L)       a variant: one field of dom φ, with a value for it
```

with the abbreviations

```
1            := rec ∅                                    the unit
0            := var ∅                                    the empty type
t₁ × t₂      := rec { fst ↦ t₁, snd ↦ t₂ }
t₁ + t₂      := var { inl ↦ t₁, inr ↦ t₂ }
optional t   := var { none ↦ 1, some ↦ t }
enum c₁…cₙ   := var { c₁ ↦ 1, …, cₙ ↦ 1 }
```

`lbls(t)` is the set of labels occurring in `t`. A type is **label-free** when `lbls(t) = ∅`, and **primitive-free**
when no `prim p` occurs in it.

**Proposition 3.3 (binary form; here).** Fix a total order on `N`. Translate `rec {n₁ ↦ t₁, …, nₖ ↦ tₖ}`, with
`n₁ < … < nₖ`, to `⌜t₁⌝ × (⌜t₂⌝ × ( … × (⌜tₖ⌝ × 1)))`, and `var` likewise with `+` and `0`.

1. **Values.** `V_E(t) ≅ V_E(⌜t⌝)` for every family `E`, naturally in `E` (section 5).
2. **Terms.** Each mapping term of definition 8.5 translates to the binary grammar: access to the `i`-th field in the
   order to `i − 1` second projections followed by a first, injection at the `i`-th field to `i − 1` right injections
   followed by a left, and a `case` to nested binary cases. Evaluation commutes with the translation and the
   isomorphisms of part 1.

Every definition and result of the paper, stated over its grammar `t ::= 0 | 1 | t + t | t × t | Prim p | Lbl l`,
therefore holds over `Ty(L)` up to the isomorphisms of part 1. They are isomorphisms, not equalities: a record of one
field `n ↦ t` translates to `t × 1`.

*Proof.* Part 1 by induction on `t`, using the associativity and unit isomorphisms of `×` and `+` in `Set`, which are
natural. Part 2 by induction on the term, using part 1 at each constructor.

## 4. Schemas

**Definition 4.1 (APG schema; paper §3).** An APG schema is `S = (L, σ)`: a set `L` of labels and a function
`σ : L → Ty(L)` giving each label its type. A schema is **finite** when `L` is finite.

A type may name labels, its own included: `σ(A) = A + 1` is a schema of streams (paper §4.3). A label inside a type is
a reference to an element, a leaf of the grammar, so a schema's recursion runs through element identity and never
through the unfolding of a type.

**Definition 4.2 (element kinds; paper §4.2).** A label `l` is:

- a **vertex label** when `σ(l) = 1`;
- an **edge label** when `σ(l)` is a record of exactly two fields, each `lbl a` of a vertex label `a`: the paper's
  `lbl a × lbl b`, with fields of any names;
- a **vertex property** when `σ(l)` is a record of exactly two fields, one `lbl a` of a vertex label and one of a
  label-free type, and an edge property or a meta-property likewise, with `a` an edge or a property label;
- a **meta-edge label** when `σ(l)` has a field of a label that is no vertex label (paper §4.2);
- a **hyperedge label** (here), generalising the edge, when `σ(l)` is a record whose fields include two or more labels
  or optional labels, as the paper's example of trips is (paper §4.1).

The kinds classify the schema; they add no structure to it.

## 5. Values

**Definition 5.1 (values; paper §3, equation 1).** For `E ∈ Set^L`, `V_E : Ty(L) → Set` is defined by recursion on
types:

```
V_E(prim p) = PV(p)
V_E(lbl l)  = E(l)
V_E(rec φ)  = ∏_{n ∈ dom φ} V_E(φ(n))                 one value per field
V_E(var φ)  = ∑_{n ∈ dom φ} V_E(φ(n)) = { (n, v) | n ∈ dom φ, v ∈ V_E(φ(n)) }
```

So `V_E(1)` has one value, the empty record, and `V_E(0)` is empty.

**Definition 5.2 (action on families).** A morphism `h : E → E'` of `Set^L` gives `V_h(t) : V_E(t) → V_{E'}(t)`: the
identity on `prim p`, `h_l` on `lbl l`, field by field on a record, and `(n, v) ↦ (n, V_h(φ(n))(v))` on a variant.

**Proposition 5.3 (here).** For every `t`, `V_(−)(t)` is a functor `Set^L → Set`: `V_id(t) = id`, and
`V_{h'∘h}(t) = V_{h'}(t) ∘ V_h(t)`. *Proof.* Induction on `t`.

**Definition 5.4 (value terms; paper §3.1).** For a set `E` of elements labelled by `λ : E → L`:

```
v ::= ⟨a⟩_p                     a primitive value, a ∈ PV(p)
    | elem e                    a reference to the element e ∈ E
    | { n₁ = v₁, …, nₖ = vₖ }   a record, the nᵢ distinct
    | n · v                     the variant field n with the value v
```

A value term is checked against a type:

- `⟨a⟩_p : prim p` when `a ∈ PV(p)`;
- `elem e : lbl l` when `λ(e) = l`;
- `{ n₁ = v₁, …, nₖ = vₖ } : rec φ` when `dom φ = {n₁, …, nₖ}` and `vᵢ : φ(nᵢ)` for each `i`;
- `n · v : var φ` when `n ∈ dom φ` and `v : φ(n)`.

`v : t` holds exactly when `v` denotes a member of `V_E(t)`, with `E(l) = λ⁻¹(l)`. A variant term is checked, not
inferred: `n · v` checks against every variant type whose field `n` admits `v`, which is why the paper annotates its
injections with the other summand.

**Definition 5.5 (references).** `refs(v)`, the elements a value term references, is the set of `e` with `elem e`
occurring in `v`. It is finite, since a term is.

## 6. Algebraic property graphs

**Definition 6.1 (APG; paper §3).** An APG on a schema `S = (L, σ)` is `G = (E, υ)`:

- `E ∈ Set^L`, the elements of each label;
- `υ_l : E(l) → V_E(σ(l))` for each `l ∈ L`, the value of each element.

**Definition 6.2 (single-set form; paper §3.1).** Equivalently, `G = (E, λ, υ)`: a set `E` of elements, a labelling
`λ : E → L`, and `υ`, sending each element to a value term, such that the type each value is checked against is its
label's:

```
υ(e) : σ(λ(e))      for every e ∈ E
```

which is the paper's commuting square `τ ∘ υ = σ ∘ λ`, with `τ` the type of a value.

**Proposition 6.3 (paper §3.1).** The two forms are in bijection: `E(l) := λ⁻¹(l)` and `υ_l := υ|E(l)` one way, and
`E := ⨆_{l ∈ L} E(l)` with `λ` and `υ` read off the summands the other.

**Definition 6.4.** An APG is **finite** when `E` is finite. A finite APG is a finite set of labelled elements, each
carrying one finite value term: the shape a store holds.

**Definition 6.5 (element graph).** The element graph of `G` has the elements of `G` as its vertices and an arc from
`e` to `e'` for each occurrence of `elem e'` in `υ(e)`, named by the path of field names leading to the occurrence.

**Proposition 6.6 (referential integrity; here).** Every arc of the element graph ends at an element of `G`, and that
element carries the label the path's type names. *Proof.* `V_E(lbl l) = E(l)`, so a reference typed `lbl l` is an
element of `E(l)`.

The element graph may be cyclic, as a stream that returns to an earlier element is (paper §4.3): a cycle is a
reference to an existing element, never an infinite value.

**Example 6.7.** People, organisations, a membership with its own attributes, and provenance on the membership and on
a person:

```
L = { Person, Organization, Source, MemberOf, AssertedBy }

σ(Person)       = rec { name ↦ prim String }
σ(Organization) = rec { label ↦ prim String }
σ(Source)       = rec { label ↦ prim String }
σ(MemberOf)     = rec { member     ↦ lbl Person,
                        collection ↦ lbl Organization,
                        role       ↦ enum employee contractor,
                        confidence ↦ optional (prim Float64) }
σ(AssertedBy)   = rec { statement  ↦ var { membership ↦ lbl MemberOf, person ↦ lbl Person },
                        asserter   ↦ lbl Source }
```

An APG on it:

```
E(Person) = { ada }   E(Organization) = { acme }   E(Source) = { hr }
E(MemberOf) = { m1 }  E(AssertedBy) = { a1, a2 }

υ(ada)  = { name = ⟨"Ada"⟩ }
υ(acme) = { label = ⟨"Acme"⟩ }
υ(hr)   = { label = ⟨"HR system"⟩ }
υ(m1)   = { member = elem ada, collection = elem acme, role = employee · {}, confidence = some · ⟨0.98⟩ }
υ(a1)   = { statement = membership · elem m1, asserter = elem hr }
υ(a2)   = { statement = person · elem ada,    asserter = elem hr }
```

`MemberOf` is a hyperedge label with attributes of its own; `m1` has identity, so a second membership between Ada and
Acme is a second element; `AssertedBy`'s `statement` field is a role whose type is a union of labels; and `AssertedBy`
is a meta-edge label, so `a1` is a relation over a relation, an element whose value references another relation's
element.

## 7. Homomorphisms

**Definition 7.1 (homomorphism; paper §5.2).** A homomorphism `h : G → G'` of APGs on one schema `S` is a morphism
`h : E → E'` of `Set^L` that commutes with the values:

```
υ'_l ∘ h_l  =  V_h(σ(l)) ∘ υ_l        for every l ∈ L
```

It sends each element to an element of the same label, and its image's value is the element's value with every
reference mapped by `h`; primitive values are preserved exactly. CQL checks the same two conditions on its transforms,
the labels agreeing and the data agreeing through the element map; its transforms also carry a label map between two
schemas, and definition 7.1 is the case of the identity label map.

**Proposition 7.2 (paper §5.1).** APGs on `S` and their homomorphisms form a category, `S-APG`, with componentwise
identities and composition.

**Definition 7.3.** An isomorphism is a homomorphism whose components are bijections. Two APGs are isomorphic exactly
when they differ by a renaming of their elements, which is how part 3 compares two APGs whose element names differ.

## 8. Equivalent formulations

### 8.1 Coalgebras

**Proposition 8.1 (paper §5.1).** A schema `S` determines the functor `F_S : Set^L → Set^L`,
`F_S(E)(l) = V_E(σ(l))` and `F_S(h)_l = V_h(σ(l))`. An APG on `S` is exactly an `F_S`-coalgebra `υ : E → F_S(E)`, and
a homomorphism exactly a coalgebra morphism, so `S-APG = F_S-Coalg`. `F_S` is a polynomial functor (paper §8).

### 8.2 Models of the APG theory

**Definition 8.2 (APG theory; paper §5.2).** `C_S` is the free category with finite products and finite coproducts,
terminal and initial objects included, generated by the objects `lbl l` for `l ∈ L` and `prim p` for `p ∈ P`, the
arrows `f : prim p₁ × … × prim pₖ → prim p` for the typeside's functions, and an arrow `δ_l : lbl l → σ(l)` for each
label (paper footnote 12, after Cockett and Seely). Its objects are exactly the types of `Ty(L)`, in binary form.

**Definition 8.3 (model).** A model of `C_S` is a functor `M : C_S → Set` that preserves finite products and
coproducts and sends `prim p` to `PV(p)` and `f` to `⟦f⟧`. A morphism of models is a natural transformation `η` with
`η_{prim p} = id`, `η_{t × u} = η_t × η_u` and `η_{t + u} = η_t + η_u`.

**Proposition 8.4 (paper §5.2).** `G = (E, υ)` gives the model `M(t) = V_E(t)`, `M(δ_l) = υ_l`, and a model `M` gives
the APG `E(l) = M(lbl l)`, `υ_l = M(δ_l)`. The category of models is equivalent to `S-APG`.

**Distributivity (decided 2026-11-08).** `C_S` is taken **distributive**: the canonical maps
`t × u + t × v → t × (u + v)` and `0 → t × 0` are isomorphisms. The paper allows the choice (paper §5.2), writing the
second map as `t → t + 0`, which is an isomorphism in every category with an initial object; `0 → t × 0` is the one
distributivity requires.

- Models are unchanged, since `Set` is distributive (paper §5.2).
- The mapping terms of section 8.3 need it: a `case` whose branches read the argument `x` is an arrow
  `τ × (A + B) → C`, which factors through `τ × A + τ × B` only when the first canonical map is invertible.
- More schema mappings exist (section 9).
- The paper notes that distributivity complicates term rewriting (paper §5.2); section 21 records how equality of
  mapping terms is checked until a feature needs it decided.

### 8.3 Mapping terms

**Definition 8.5 (mapping terms).** An arrow of `C_S` out of a type `τ` is written as a term in one variable `x : τ`:

```
s ::= x                                        the argument
    | ⟨a⟩_p                                    a primitive value
    | f(s₁, …, sₖ)                             a typeside function
    | { n₁ = s₁, …, nₖ = sₖ }                  a record
    | s . n                                    a record's field
    | n · s                                    a variant injection
    | case s of { n₁ y₁ ⇒ s₁ | … | nₖ yₖ ⇒ sₖ }   one branch per field of the variant
    | absurd s                                 out of the empty type
    | deref s                                  an element's value: s : lbl l gives deref s : σ(l)
```

typed by these rules: `x : τ`; `⟨a⟩_p : prim p`; `f(s₁, …, sₖ) : p` for `f : p₁ … pₖ → p` and `sᵢ : prim pᵢ`;
`{ n₁ = s₁, …, nₖ = sₖ } : rec φ` when `dom φ = {n₁, …, nₖ}` and `sᵢ : φ(nᵢ)`; `s . n : φ(n)` for `s : rec φ`;
`n · s : var φ` for `s : φ(n)`; a `case` on `s : var φ` has a branch per field of `dom φ`, each binding
`yᵢ : φ(nᵢ)` and every branch of one type, and its branches may also read `x` (the distributivity of section 8.2);
`absurd s : t` for `s : 0`; and `deref s : σ(l)` for `s : lbl l`, the generator `δ_l`. Two terms are equal when the
laws of a distributive category with finite products and coproducts, β and η for records and variants, prove them
equal.

**Definition 8.6 (evaluation).** For an APG `G`, a term `x : τ ⊢ s : t` denotes `⟦s⟧_G : V_E(τ) → V_E(t)`, by
recursion on `s`, with `⟦deref s⟧_G = υ_l ∘ ⟦s⟧_G`.

**Proposition 8.7 (here).** Evaluation terminates on every APG, cyclic ones included. *Proof.* `⟦·⟧` recurses on the
term, which is finite; `deref` returns an element's stored value and evaluates nothing further.

## 9. Schema mappings and Δ

**Definition 9.1 (schema mapping; paper §7.1).** A mapping `Φ : S → S'` gives each label `l ∈ L`:

- a type `τ_l ∈ Ty(L')`, the label's image;
- a mapping term `x : τ_l ⊢ t_l : Φ̂(σ(l))`,

where `Φ̂ : Ty(L) → Ty(L')` replaces each `lbl l₀` by `τ_{l₀}` and is the identity on primitive types and structural
on records and variants. This is the paper's functor `C_S → C_{S'}` preserving products, coproducts and primitive
types, given by its values on the generators, `lbl l ↦ τ_l` and `δ_l ↦ t_l`; CQL writes it the same way, a type and a
term per label, and checks the term against `Φ̂(σ(l))`. The identity mapping has `τ_l = lbl l` and `t_l = deref x`.

**Definition 9.2 (Δ; paper §7.1).** For an APG `G' = (E', υ')` on `S'`, `Δ_Φ(G') = (E, υ)` on `S` is

```
E(l)   = V_{E'}(τ_l)
υ_l(v) = ι_{σ(l)}( ⟦t_l⟧_{G'}(v) )
```

where `ι_t : V_{E'}(Φ̂(t)) → V_E(t)` is the bijection that is the identity at labels, since
`V_{E'}(Φ̂(lbl l₀)) = V_{E'}(τ_{l₀}) = E(l₀)`, and is structural elsewhere. On a homomorphism,
`Δ_Φ(h)_l = V_h(τ_l)`.

An element of `Δ_Φ(G')(l)` is a value of `τ_l` over `G'`; its identity is that value, tagged by `l` in the single-set
form of definition 6.2, since two labels with one image hold equal values. CQL tags it the same way.

**Proposition 9.3 (paper §7.1).** `Δ_Φ` is a functor `S'-APG → S-APG`, and mappings compose as functors of the APG
theories, so `S ↦ S-APG` and `Φ ↦ Δ_Φ` form a functor `Schema^op → Cat`. The migrations `Δ` expresses include
dropping labels, duplicating labels, and permuting and computing fields (paper §7.1). The paper adds joins of labels
given equality functions; under definition 9.2 an image that is a record of labels yields every combination, and
keeping the matching ones is a selection, which `Δ` alone does not perform (after proposition 9.5).

**Proposition 9.4 (finiteness; here).** If `G'` is finite and every `τ_l` holds no `list` and only primitive types
with finitely many values, `Δ_Φ(G')` is finite. *Proof.* `V_{E'}(t)` is finite for such a `t` over finite `E'`, by
induction on `t`: a `set t` over a finite `V_{E'}(t)` has as many values as `V_{E'}(t)` has subsets. A `list t` over a
type with a value has infinitely many values, one per length. A primitive type with infinitely many values in `τ_l`
may make `E(l)` infinite; it need not, as when it sits in a record beside a label with no elements.

**Finite images (decided 2026-11-08).** A mapping's label images are finite types: no `list` occurs in `τ_l`, and
every primitive type in it has finitely many values, as `Bool` and every enumeration have; a mapping with another
image is refused, since `Δ` along it may be infinite. The test is on the type alone. The images of `SCHEMA-EVOLUTION.md`'s fragment, labels and
variants of labels, pass it; a finite primitive still serves, as an image `lbl A × enum x y` splits each element into
two tagged copies.

**Proposition 9.5 (one origin; here).** If every `τ_l` is a label `lbl l'` or a variant of labels
`var { n₁ ↦ lbl l'₁, …, nₖ ↦ lbl l'ₖ }`, then each element of `Δ_Φ(G')(l)` is one element `e'` of `G'`, its
**origin**: `e'` itself under a label image, `(nᵢ, e')` under a variant. So `|Δ_Φ(G')(l)| = |E'(l')|`, or
`|E'(l'₁)| + … + |E'(l'ₖ)|`. *Proof.* By definition 5.1, `V_{E'}(lbl l') = E'(l')` and
`V_{E'}(var {nᵢ ↦ lbl l'ᵢ}) = ∑ᵢ E'(l'ᵢ)`.

A record of labels as an image, `lbl a × lbl b`, makes every pair of an `a` and a `b` an element: a join, whose size
is the product of theirs. Proposition 9.5 is the core of `SCHEMA-EVOLUTION.md`'s non-merging fragment: a target label
fed by a union of source labels is a `Δ` along a variant of labels, each image with one origin. The fragment adds two
steps `Δ` does not take: a `where`, a selection, which is an equalizer of homomorphisms; and a role assigned by a key
lookup, which searches a label's elements by key and is defined by the key's semantics (section 10). Part 3 defines
both.

**Example 9.6.** A target schema renames `MemberOf` of example 6.7 to `Membership` and drops its `confidence`. Its
labels are example 6.7's with `Membership` for `MemberOf`, and its types differ in two:

```
σ'(Membership) = rec { member ↦ lbl Person, collection ↦ lbl Organization, role ↦ enum employee contractor }
σ'(AssertedBy) = rec { statement ↦ var { membership ↦ lbl Membership, person ↦ lbl Person },
                       asserter  ↦ lbl Source }
```

The mapping runs from the target to the source, since `Δ` carries source data along it:

```
τ_Membership = lbl MemberOf
t_Membership = { member     = (deref x) . member,
                 collection = (deref x) . collection,
                 role       = (deref x) . role }
```

with the identity mapping on every other label, which type-checks for `AssertedBy` because `Φ̂(σ'(AssertedBy))` is
example 6.7's `σ(AssertedBy)`. `Δ` gives one `Membership` per `MemberOf`, `m1`'s image carrying Ada, Acme and the role,
and every `AssertedBy` whose statement names `m1` names its image.

## 10. The model and the views

Decided 2026-11-08, after comparing the APG model with PG-Schema, the LDBC proposal for property graph schemas:

| Layer | Formalism | Carries |
|---|---|---|
| Foundation | The APG model of this file | Every instance's identity, n-ary relations, relations over relations, UQL's result types, and migrations as `Δ` (`SCHEMA-EVOLUTION.md`) |
| GQL view | A closed graph type of ISO GQL, read with PG-Schema's STRICT, closed semantics | The generated graph type and its conformance, which MGQL checks for the GQL core |
| Constraints | PG-Keys for keys, uniqueness, presence and single values; PG-Schema's proposed `COUNT` form for participation bounds; UGS's own rules for acyclicity, no-overlap, limits and bitemporal statements | Constraints on the APGs of a schema, defined in part 3 |
| RDF view | RDF 1.2 with reifiers, published with SHACL shapes | Part 3's construction; the shapes cross-checked against the common core of SHACL, ShEx and PG-Schema |

**Why APG is the foundation.**

- PG-Schema's property graph maps each edge to an ordered pair of nodes (PG-Schema, definition 3.1): an edge joins two
  nodes and never ends at an edge. UGS's n-ary relations and relations over relations, DeepBrain's provenance on every
  statement among them, are nodes there, the encoding the GQL view performs; as the foundation, that encoding would
  become the definition.
- PG-Schema types structurally: an element conforms to every type whose label set equals its own and whose record
  admits its properties (PG-Schema, definition 4.2), so it has several types or none. UGS types nominally, one concrete
  type per instance recorded in its ID's prefix, as an APG labels each element once.
- PG-Schema's property values are atomic. Records, variants and optional values are the APG model's types, and UQL's
  typed trees, nested records with `Option` and tagged unions for `on Type`, are its values.
- PG-Schema has no schema mappings or migration. The APG model has `Δ` (section 9).
- PG-Schema carries what the APG model lacks, keys and constraints with a formal semantics, and is LDBC's input to the
  next edition of ISO GQL. It therefore describes the GQL view and lends its keys.

**The GQL view's graph type.** A node of PG-Schema's closed type conforms only when its label set equals the type's
exactly (PG-Schema, section 4.2), so a closed supertype admits no node of a subtype, which carries the subtype's label
as well. The generated graph type therefore takes one node type per concrete type, with the exact label set of the
type and every supertype, and one edge type per edge-encoded relation, role edge and navigation edge, its endpoints
listing the node types of every concrete type its role admits. A supertype is a label, never a node type of its own.

**Keys.** A UGS key is a PG-Keys `IDENTIFIER` key, exclusive, mandatory and singleton, whose scope is the key's type
tree in one graph; `@unique` is an `EXCLUSIVE` key; a required attribute or role is `MANDATORY` and a single-valued one
`SINGLETON`. Each is checked as the emptiness of its violation query, as PG-Keys checks it (PG-Keys, section 4.5). A
role assigned by key lookup in a migration reaches at most one element because the key is exclusive.

## 11. UGS constructs in the model

Every construct of `SCHEMA.md` has its place in the model, defined in the section named:

| UGS construct | In the model | Section |
|---|---|---|
| Entity type, attribute, optional attribute | A label whose type is the record of its fields, own and inherited; an optional field `optional t` | 13.2 |
| Default | A write-time rule: the stored value of an omitted field is its default | 13.2 |
| Enum | `enum`, a variant of units | 13.2 |
| Interval, measure, record attribute | A record of primitive types | 13.2 |
| Catalog type as a value or participant | An enumeration of the concrete type names it admits | 13.2, 13.3 |
| Single inheritance, abstract types, type tests | One label per concrete type; a supertype is the variant of its concrete subtypes | 13.1 |
| Relation instance with identity, n-ary, with attributes | A hyperedge label: the record of its single-valued roles and attributes | 13.3 |
| Union-typed role, a role typed by a supertype | A variant over the concrete labels it admits | 13.3 |
| Relation over a relation | A field typed by a relation's label, a meta-edge | 13.3 |
| `@set`, `@multi` | Identity for every element; `@set` a key on the roles | 13.3, 14.2 |
| List and set attributes | `list t` and `set t` over label-free types | 12.2 |
| Set-valued role | Reification: one element of a role label per participant | 12.3 |
| `@symmetric` relation | A record of two fields read in either order | 12.4 |
| `@open` data and `Resource` | The label `Resource`, and undeclared triples as elements whose objects are values | 13.4 |
| Derived names, functions | Queries over the APG, outside its schema | 13.5 |
| Keys, `@unique`, cardinality, checks, `@acyclic`, `@no_overlap`, `@limit` | State constraints | 14.2 |
| Type fixed, keys and `@immutable` unchanged, `@append_only`, delete and retraction actions | Transition constraints | 14.3 |
| Instance IRIs, an edge-encoded relation without a reifier | The naming `ι`, partial: such an edge is named by its triple | 15.1 |
| Named graphs | One APG per graph; a working graph as the union of a graph and the graphs its elements reference | 15.2 |
| Bitemporal versions, `asserted`, `live` | States indexed by commit, and their slices | 15.3 |
| A node's labels in the GQL view | The exact label set of the concrete type and its supertypes | 10, 18 |

## 12. Collections

### 12.1 The decision

UGS has collections of two kinds: collections of **values**, its `List` and `Set` attributes, and collections of
**references**, its set-valued roles. Collections of values enter the types as `list t` and `set t` over label-free
types; collections of references are reified, one element per member.

- UGS's collections of values never hold a reference: a `List` or `Set` holds scalars, enums, `Iri`, measures,
  intervals or records, and an `Iri` is a value (`SCHEMA.md` sections 5.1 and 5.4).
- Over a label-free type, values do not depend on the elements (proposition 12.2), so the two constructors enter `F_S`
  as constants: `F_S` stays polynomial, and the theory `C_S`, `Δ` and every proposition of part 1 hold unchanged.
- A collection of references would put the finite subsets of `E(l)` into `F_S`, which is then not polynomial, and
  `Δ`'s mapping terms would need comprehensions over collection monads (paper §6.2; Grust). Reification keeps both
  out.
- Both choices follow the store and the views. A `List` is one literal; a `Set` is one triple per member; a set-valued
  role is one role triple per participant (`SCHEMA.md` sections 5.1 and 11.1), each the RDF view of one reified
  element.

### 12.2 Collections of values

**Definition 12.1 (collection types).** Definition 3.2 gains two constructors:

```
list t     for a label-free t without a collection in it      a finite sequence
set t      for t a primitive type or an enum                  a finite set
```

as UGS admits them: a `List` of a scalar, enum, record, `Iri`, measure or interval and no nested collection, and a
`Set` of what compares as a scalar (`SCHEMA.md` section 5.4, `UGS0205`). Their values are

```
V_E(list t) = the finite sequences of V_E(t)
V_E(set t)  = the finite subsets of V_E(t)
```

two lists equal element by element and two sets extensionally, and `V_h(list t) = V_h(set t) = id`.

**Proposition 12.2 (here).** For a label-free `t`, `V_E(t) = V_∅(t)` for every family `E`, and `V_h(t) = id` for every
`h`. *Proof.* Induction on `t`: no `lbl l` leaf occurs, and every other constructor's values are built from its
arguments' alone.

**Corollary 12.3 (here).** With collections, `F_S(E)(l) = V_E(σ(l))` remains a polynomial functor, each label-free
subterm of `σ(l)` contributing a constant, and propositions 5.3, 6.3, 6.6, 7.2, 8.1 and 8.4 hold with unchanged
proofs. In `C_S` the collection types are further primitive types of an extended typeside: `P*` is the least set
holding `P` and `list t` and `set t` for every admitted `t` over `P*`, and the collection operations are typeside
functions:

| Type | Functions |
|---|---|
| `list t` | `nil`, `cons`, `append`, `length`, `at`, giving `optional t` |
| `set t` | `empty`, `insert`, `union`, `member`, giving `enum false true`, and `size` |

A mapping term reads and builds collections through these functions, and transforms their members through the two
forms of definition 12.4.

**Definition 12.4 (map and filter; decided 2026-11-08).** For `s : list t` or `s : set t`, the mapping-term forms

```
map(s, y ⇒ u)        u : t', the image of the member y
filter(s, y ⇒ q)     q : enum false true, whether the member y is kept
```

take a body that reads the member `y` alone: no other variable, and no `deref`. `map` applies `u` to every member, in
order for a list; over a set it gives the set of the images, so members with equal images become one, and `t'` must be
a type a set admits. `filter` keeps the members for which `q` is `true`. The body is then a closed arrow `t → t'` of
`C_S` between label-free types, so `map` is the action of `list` and `set` on that arrow: `map(s, y ⇒ y) = s` and
`map(map(s, y ⇒ u), y ⇒ u') = map(s, y ⇒ u'[u/y])`, equations the theory of section 8.3 gains. Proposition 12.2 and
corollary 12.3 hold unchanged, since no reference enters a collection.

UQL's result types are label-free types of this grammar, a list of records among them (`../query/QUERY-ULTRA-QL.md`
section 5).

### 12.3 Collections of references

**Definition 12.5 (role label).** For a relation label `R` with a set-valued role `r` whose participants have the type
`U`, the schema has the **role label** `R_r`, and `σ(R)` has no field `r`:

```
σ(R_r) = rec { instance ↦ lbl R, participant ↦ ⟦U⟧ }
```

with `⟦U⟧` the role's participant type of section 13.3. Each participant of `r` in an instance is one element of
`R_r`, whose value names the instance and the participant.

Two constraints of section 14.2 complete the role: no two elements of `R_r` carry one value, as a set has no duplicate
member; and every element of `R` has at least one element of `R_r`, as a set-valued role holds one participant or more
(`UGS1016`). An element of a role label has no IRI: the pair of its instance and its participant names it (section
15.1).

### 12.4 Symmetric relations

**Definition 12.6.** A `@symmetric` relation's label has the type of a binary relation, a record whose two role fields
`a` and `b` have one participant type, and its elements are read **up to the swap** of `a` and `b`:

- its navigation name, which both roles share, reaches the participant in the other field from either one;
- a `@set` symmetric relation's key is the unordered pair of its participants (section 14.2);
- the views present it without a direction (sections 17, 18).

The model fixes neither order of a value's two participants. The store fixes one, the order of the participants' IDs
(`SCHEMA.md` section 6.4), which no view and no query observes.

### 12.5 Unnesting collections of values

**Definition 12.7 (unnest; decided 2026-11-08).** For a label `l` with a field `f` of type `set t` or `list t`, the
unnest of `G` along `l.f` adds the derived label `l_f` to `G`'s schema,

```
σ(l_f) = rec { origin ↦ lbl l, member ↦ t }                       for set t
σ(l_f) = rec { origin ↦ lbl l, position ↦ prim UInt32, member ↦ t }  for list t
```

with one element per member of `e.f` for every element `e` of `l`, per position for a list, its value naming `e` and
the member. It is the reification of definition 12.5 applied to a collection of values: the element is named by its
origin and member, or position, and holds no data beyond them. The unnest is a function of `G`, defined on every APG;
`Δ` along an image `lbl l_f` (proposition 9.5) then gives one target element per member, each with one origin, which
is how a migration turns a collection's members into instances (`SCHEMA-EVOLUTION.md` section 3.2).

## 13. The schema of a catalog

A catalog version `K` of a tenant denotes an APG schema `S(K) = (L, σ)` with constraints `C(K)` (section 14). This
section defines `S(K)`.

### 13.1 Labels and inheritance

`L` holds one label for each concrete entity type, each concrete relation type, each role label of section 12.3,
and `Resource` and `Undeclared` where `@open` admits undeclared data (section 13.4), each qualified by its namespace.
An abstract type is no label.

For a type `T`, `conc(T)` is the set of concrete types at or below `T`, `T` itself included when it is concrete, and

```
⟦T⟧ = lbl T                                  when conc(T) = {T}
⟦T⟧ = var { C ↦ lbl C  |  C ∈ conc(T) }      otherwise
```

A concrete type's fields are its own and every supertype's. A specialized role holds its participant in one field,
under its own name; the super role's name reads it, in the views (`SCHEMA.md` section 11.2) and in queries.

**Proposition 13.1 (here).** Every element carries one label, its concrete type, and `is T` holds of an element exactly
when its label is in `conc(T)`, which the prefix-range check of `SCHEMA.md` section 7.1 decides. *Proof.* An instance
has one concrete type, fixed for life (section 14.3), and its labels in the views are derived from it (section 18).

### 13.2 Attributes

An attribute's UGS type `T` translates to `⟦T⟧`; an optional attribute to `optional ⟦T⟧`:

| UGS type | `⟦T⟧` |
|---|---|
| `Bool`, the integer and float types, `Decimal(p, s)`, `String`, `Text`, `Bytes`, `Date`, `Time`, `DateTime`, `LocalDateTime`, `Duration`, `Iri`, `Vector<…>`, `Geo`, `Unit`, `QuantityKind`, `Measure<K, u>` | A primitive type of the typeside, with `SCHEMA.md` section 5.1's values; a fixed-unit measure is its number in the unit |
| `enum E { m₁, … }` | `enum m₁ …` |
| `record N { f: T, … }` | `rec { f ↦ ⟦T⟧, … }`, an optional field `optional ⟦T⟧` |
| `Interval<T>`, `T` temporal | `rec { end ↦ optional ⟦T⟧, end_precision ↦ optional (prim Precision), start ↦ optional ⟦T⟧, start_precision ↦ optional (prim Precision) }`, a precision present exactly when its bound is, and at least one bound present (`SCHEMA.md` section 5.5) |
| `Interval<T>`, `T` an integer or decimal type | `rec { end ↦ optional ⟦T⟧, start ↦ optional ⟦T⟧ }`, its bounds exact, at least one present |
| `Measure<K>`, `Measure` | `rec { value ↦ prim Float64, unit ↦ prim Unit }` |
| `List<T>`, `Set<T>` | `list ⟦T⟧`, `set ⟦T⟧` (section 12.2) |
| `schema.EntityType<T>`, `schema.RelationType<T>` as a value | `enum` of the names of `conc(T)`: a catalog type is a value here, never a reference |

`σ(T)` of a concrete entity type is the record of its fields under their names; an `@bitemporal` type's has the field
`asserted ↦ rec { start ↦ prim DateTime, end ↦ optional (prim DateTime) }` too, which the engine writes (section 15.3).
A default is a write-time rule: a field omitted at insert is stored with its default, so the model holds the default
as the value.

### 13.3 Relations

For a concrete relation type `R`:

```
σ(R) = rec { r ↦ ⟦U_r⟧              for each required, single-valued role r,
             r ↦ optional ⟦U_r⟧     for each optional role r,
             a ↦ ⟦T_a⟧              for each attribute a, as section 13.2 }
```

where `U_r` is the role's participant union `A | B | …` and `⟦U_r⟧` the variant over `conc(A) ∪ conc(B) ∪ …`, or
`lbl C` when that set is the one type `C`. A set-valued role has its role label instead (section 12.3).

- **Relations over relations.** A role whose union admits a relation type, directly or through a statement type
  covering it, references that relation's elements: the paper's meta-edge (definition 4.2).
- **Catalog types as participants.** A role typed `schema.EntityType<T>` is a value field, the enumeration of section
  13.2.
- **Identity.** Every element of `R` has identity, so a `@multi` relation's two instances between the same participants
  are two elements with equal values; a `@set` relation, the default, admits at most one live element per combination
  of participants, a key of section 14.2.
- **Navigation names.** A navigation name is no field. The navigation from a participant `p` along role `r` reaches
  the elements `{ e ∈ E(R) | e.r = p }`, or those of `R_r` naming `p`, and its cardinality bounds their number (section
  14.2).
- **Encodings.** Whether `R` is edge- or node-encoded (`SCHEMA.md` section 11.1) is the store's and the views' matter;
  the model has one form for both.

### 13.4 Undeclared data

Where `@open` admits undeclared data, the schema has the labels

```
σ(Resource)   = 1
σ(Undeclared) = rec { subject ↦ ⟦any⟧, predicate ↦ prim Iri, object ↦ prim Term }
```

with `⟦any⟧` the variant over every label of `L` but the role labels and `Undeclared`, and `prim Term` the RDF terms,
IRIs, blank nodes and literals, as values. A subject of no declared type is an element of `Resource`; each undeclared
triple, on a `Resource` or on an instance of a type declared `@open`, is an element of `Undeclared`. An undeclared
triple's object is a value, never a reference: an ungoverned link names an IRI and makes no claim that an element
carries it, as RDF's open world has it. A tenant without a schema has no APG; its data is the quads
`../binary_protocol/PROTOCOL.md` section 11.13 governs.

### 13.5 Derived names and functions

A derivation `derive T.n = body` (`SCHEMA.md` section 6.8) is no field of `σ(T)`. It denotes a function `d_n(G, e)` of
an APG `G` and an element `e` of a label in `conc(T)`, computed by its body: navigation steps forward and inverse,
filters, a bound `+` or `{m, n}` read as reachability, and `exists`; or, for a computed derivation, an expression over
`e`'s own value and the built-in fields of `SCHEMA.md` section 5.11, which reads no other element, so `d_n(G, e)`
depends on `e` alone. `@materialized` stores a cache that must equal `d_n` at every state; `@virtual` computes it when
read. A function (`SCHEMA.md` section 6.9) is a function of `G` and its parameters likewise. A transitive closure is no
arrow of `C_S`, so derivations and functions are queries over the APG; an extended mapping term calls them as reads of
its source (definition 16.3).

## 14. Constraints

### 14.1 Form

`C(K)` holds two kinds of constraint. A **state constraint** is a predicate on one APG `G` on `S(K)`, the graph at a
read point, read as the slice of section 15.3. A **transition constraint** is a predicate on a pair `(G, G')` of
consecutive states, the APGs before and after one commit. The data is valid when every state satisfies the state
constraints and every commit the transition constraints. Each constraint has a violation query, which returns the
elements that break it, and holds when the query returns nothing, as PG-Keys validates (PG-Keys, section 4.5); the
refusal it raises is `SCHEMA.md` section 8.7's.

### 14.2 State constraints

In the table, `E_g(T)` is `⋃ { E_g(C) | C ∈ conc(T) }` within one graph `g`, and `e.f` the field `f` of `e`'s value:

| UGS | The constraint on `G` | PG-Keys or PG-Schema form |
|---|---|---|
| `@key(k₁, …)` on a tree `T` | `e ↦ (e.k₁, …)` is injective on `E_g(T)`, the retracted elements a state holds included (`SCHEMA.md` section 7.2) | `IDENTIFIER`: `EXCLUSIVE`, with `MANDATORY` and `SINGLETON` holding by type, since key fields are required and single-valued |
| `@unique a` | `e ↦ e.a` is injective on the elements where `a` is present, retracted ones included | `EXCLUSIVE` |
| Required, single-valued attribute or role | Holds by type: the field is present once in every value | `MANDATORY`, `SINGLETON`, discharged by typing |
| Set-valued role, non-empty | Every `e ∈ E(R)` is named by at least one element of `R_r` | `MANDATORY` |
| `@set` | `e ↦ (e.r₁, …)` over the single-valued roles, with each set-valued role's participants, is injective on the live elements of `E(R)`; for a `@symmetric` relation the pair is unordered | `EXCLUSIVE` |
| Role label | `e ↦ (e.instance, e.participant)` is injective on `E(R_r)` | `EXCLUSIVE` |
| Participation `?`, `+`, `{m, n}` | For every live participant `p` of the role's types, the number of live elements reaching `p` through the role lies in the bounds | PG-Schema's `COUNT m..n` |
| `@check`, `@range`, `@pattern`, `@values` | A predicate on each element's value | `MANDATORY` with a condition |
| `@acyclic`, `max_depth` | The references from the contained role to the container role, over the live elements and, under `@acyclic(open)`, the open ones, form no cycle, and no chain longer than `max_depth` | UGS's own |
| `@no_overlap(per: P)` | Elements agreeing on the roles `P` have pairwise disjoint valid intervals | UGS's own |
| `@limit(agg per G where c) op v` | The aggregate over each selected group's live elements satisfying `c` compares to `v` by `op`; a group is selected when its participants satisfy the conjuncts of `c` that read only their types, and the elements counted satisfy the rest | UGS's own |
| Graphs | A reference from an element lands in a graph of the referenced type's schema, the element's own or one it imports (section 15.2) | UGS's own |

A chain's length is its number of references, so `max_depth n` admits chains of at most `n` references, as `SCHEMA.md`
section 12 walks them. A `@check`, `@range`, `@pattern` or `@values` predicate and a set-valued role's non-emptiness
hold of every element a state holds, the retracted ones included; participation and `@limit` count the live elements
(`SCHEMA.md` section 8.4, rule 5). A lower bound of `@limit` has one `per` role, and selects every participant of that
role's types live and satisfying the selecting conjuncts, one no instance reaches included, whose group is empty; an upper
bound's groups are the combinations of participants, and under `type` the concrete types, its elements reach, since an
empty group satisfies it (`SCHEMA.md` section 8.6).

Datatypes, enum members, units of a measure's kind and vector dimensions hold by typing, since each value lies in
`V_E` of its field's type: `PV` of a primitive type holds exactly the values `SCHEMA.md` section 5.1 admits.

### 14.3 Transition constraints

For a commit from `G` to `G'`, and an element `e` of `G`:

| UGS | The constraint on `(G, G')` |
|---|---|
| Type fixed for life | `e`, if in `G'`, carries the label it carries in `G` (`UGS1004`) |
| Key immutable, `@immutable` | `e`'s key fields and `@immutable` fields are equal in `G` and `G'` (`UGS1007`, `UGS1012`) |
| `@append_only` | `e` is in `G'` with an equal value (`UGS1013`); only a legal purge removes it |
| `@bitemporal` | A change of `e`'s value is a revision: `e` stays, with a new version (section 15.3); `e` leaves the live slice only by retraction, and the store only by purge |
| `@on_delete` | When `e` is deleted, every element whose field references `e` through a role with `cascade` is deleted, with `restrict` refuses the commit (`UGS1020`), and with `orphan` has that optional field set absent; `G'` is referentially closed (proposition 6.6) |
| `@on_retract` | When `e` is retracted, every `@bitemporal` element referencing it through a role with `cascade` is retracted, with `restrict` refuses while it is live, and with `restrict open` refuses while it is open and is retracted with `e` while it is live and no longer open (`SCHEMA.md` section 8.4, rule 6) |
| Participants of `@bitemporal` relations | No deletion reaches one: every participant type of a `@bitemporal` relation is `@bitemporal` or `@append_only`, a schema rule (`UGS0602`) |
| `@retracts(r)`, `@ends(r)` | An element inserted into `G'` retracts, or closes the valid interval of, its participant in `r` in the same commit |
| Live participants | An element inserted into `G'` references no element that is retracted or deleted (`UGS1019`) |

## 15. Identity, graphs and time

### 15.1 Names

`ι`, the **naming**, sends an element to its IRI (`SCHEMA.md` section 7.3): minted from the key for a keyed element,
the handle drawn at creation for a keyless one, and the reifier for an element of
an edge-encoded relation whose type allocates one. `ι` is partial and injective where defined:

- an element of an edge-encoded relation without a reifier has no IRI; its triple, the IRIs of its anchor and its other
  participant under the anchor's navigation name, names it, and is unique because such a relation is no `@multi`
  relation, which allocates a reifier, and so has at most one live element per pair (section 14.2);
- an element of a role label has no IRI; the pair of its instance and its participant names it;
- an element a migration creates as a secondary image, of a map that is not primary or of an unnest, is keyed or takes
  a **derived handle**, a function of its origin's IRI, its target type and, for an unnest, its member
  (`SCHEMA-EVOLUTION.md` section 4.3), so every evaluation of the migration names it alike.

A homomorphism `h : G → G'` **preserves names** when `ι'(h(e)) = ι(e)` wherever `ι` is defined. System IDs implement
element identity in the store and are no part of the model.

### 15.2 Graphs

Each graph `g` of a namespace holds an APG `G_g` on `S(K)`, with elements of the labels its schema declares. Graphs
are disjoint, since every element belongs to one graph, and an element references elements of graphs whose schemas
declare the referenced types: its own namespace's or one its schema imports.

**Proposition 15.1 (here).** The union of a graph's APG and the APGs of the graphs its elements reference,
transitively, is an APG on `S(K)`: the working graph a query reads (`../query/QUERY-ULTRA-QL.md` section 2). *Proof.*
Element sets are disjoint, and the union is closed under references by construction.

### 15.3 States and slices

The database's history is a sequence of **states** `G_0, G_1, …`, one APG per commit, each with every element ever
written and not purged; a commit is a transition of section 14.3, and a read point reads one state
(`../query/QUERY-ULTRA-QL.md` section 11.1). An element of a `@bitemporal` label carries in a state its **versions**,
one per revision, each with the transaction-time interval in which it was the element's value; its `asserted` field is
the union of those intervals.

| Slice of a state | Elements | Each with |
|---|---|---|
| **Live**, the default read | Every element present and not retracted; an element of a relation that is not `@bitemporal` only while every participant of it is live (`SCHEMA.md` section 8.4, rule 6) | Its newest version |
| **Believed at** `t` | Every `@bitemporal` element whose `asserted` contains `t`'s epoch, and every element that is not `@bitemporal` and present, while every participant of it is in the slice | The version current at `t` |
| **With retracted** | Every element present | Its newest version |

**Proposition 15.2 (here).** The live slice of every state and the believed slice at every instant are referentially
closed, so each is an APG. *Proof.* By induction over commits. An insert references only live elements (`UGS1019`). A
retraction is refused while a `@bitemporal` element references the retracted one through `restrict`, refused while an
open one does through `restrict open`, which retracts the live ones that are no longer open in the same commit, and
retracts them through `cascade`; an element of a relation that is not `@bitemporal` leaves the slice with its
participant by definition. No participant of a `@bitemporal` relation is deleted, since its type is `@bitemporal` or
`@append_only` (`UGS0602`), and a deletion of any other element applies `@on_delete`, which keeps the state closed
(section 14.3). A revision keeps the element's identity, so a reference to it stays valid. The believed slice follows
the same steps read at `t`.

Two rules of `SCHEMA.md` carry the proof: `restrict open` retracts the live instances that are no longer open with
their participant, so Gen5's closed memberships and closed units leave the live slice with the organisation they name
and stay readable believed at any earlier instant; and `UGS0602` keeps every participant of a `@bitemporal` relation
from deletion.

## 16. Selections, key lookups, extended terms and coproducts

Two operations stand beside `Δ` where a migration (`SCHEMA-EVOLUTION.md` section 2) or a query needs them.

**Definition 16.1 (selection).** For an APG `G`, a set of labels and a Boolean mapping term `p` on each label's values,
the selection `G|p` keeps the elements of those labels whose values satisfy `p` and every element of the other labels.
It is **defined** when `G|p` is referentially closed: no kept element references a dropped one. Then `G|p` is an APG,
and its inclusion into `G` is an injective homomorphism that preserves names. Otherwise it is undefined, and an
operation applying it fails naming a kept element and the dropped element it references, as a migration's run fails
(`SCHEMA-EVOLUTION.md`, `UGS2202`).

**Definition 16.2 (key lookup).** For a key `k` of a type tree `T` in a graph `g`, `lookup_k` sends a key value `v` to
the element of `E_g(T)` whose key is `v`, as `optional ⟦T⟧`, absent when none has it. It is defined on the APGs
satisfying the key's exclusivity (section 14.2), which makes the element unique. A lookup reads every element of the
tree, so it is no arrow of `C_S`; a mapping term extended with lookups is evaluated on valid APGs only.

**Definition 16.3 (extended mapping terms; decided 2026-11-08).** An **extended mapping term** is a mapping term of
definition 8.5 that may also read its source APG: key lookups (definition 16.2), inverse navigations, derivations,
folds and functions (section 13.5), each evaluated on the valid source APG at one snapshot. A mapping whose terms are
all pure is a **pure mapping**, the functor of definition 9.1; one with an extended term is an **extended mapping**.
`Δ` along an extended mapping is definition 9.2 with extended terms, defined on valid APGs. It is no functor of the
APG theories, so proposition 9.3's composition law holds for pure mappings alone, and a migration report states which
kind a migration's mapping is (`SCHEMA-EVOLUTION.md` section 5).

**Proposition 16.4 (here).** `SCHEMA-EVOLUTION.md`'s non-merging fragment denotes, for a source APG `G` satisfying the
source's constraints, the target APG `(Δ_Φ G⁺)|p`: `G⁺` is `G` with the unnests its `unnest` maps name (definition
12.7), `Δ` runs along label and variant-of-label images, with extended mapping terms, and the selection of the maps'
`where` predicates follows. It is defined exactly when the selection is, and
each target element has one origin (proposition 9.5). *Proof.* Definitions 9.2, 16.1 and 16.3, with proposition 9.5
for the images.

**Definition 16.5 (coproducts; decided 2026-11-08).** The coproduct of schemas `S` and `S'` with disjoint labels is
`S + S' = (L ⊔ L', σ ⊔ σ')`, and the coproduct of an APG `G` on `S` and an APG `G'` on `S'` is the APG on `S + S'`
holding the elements of both. A migration reading two namespaces with no references between them reads their
coproduct; namespaces whose graphs reference each other's, through imports, are read as the union of
proposition 15.1, an APG on one schema whose element sets are disjoint. The model holds no other colimit: a pushout or
a coequalizer identifies elements, which UGS refuses at commit and `SCHEMA-EVOLUTION.md` refuses at a migration, so an
integration that identifies entities does so through their keys, where a conflict is refused and named.

**Proposition 16.6 (history through a migration; here).** For a migration of the fragment from a source to a target
namespace, cut over at epoch `e₂`, and an instant `t` before `e₂`, let `B_t` be the source's slice believed at `t`. Then
`(Δ_Φ B_t⁺)|p` is the target's slice believed at `t` wherever the selection is defined, an APG on the target's schema,
and an element's name in it is its name in the target's stored present. *Proof.* `B_t` is an APG (proposition 15.2), and
its key lookups find at most one element, since a key names one element for all time (`SCHEMA.md` section 7.2).
Proposition 16.4 applies to it as to the copy's source. Names agree because a primary image takes its origin's IRI and a
secondary image a name derived from its origin (section 15.1), whichever slice the origin is read from.

A query over the target that reads `believed at t` before the cutover is evaluated this way, in the target's schema,
and joins the past to the present by name (`SCHEMA-EVOLUTION.md` section 7; `query/QUERY-ULTRA-QL.md` section 11.2).

## 17. The RDF view

**Definition 17.1 (`R`).** For a valid APG `G` on `S(K)` with its naming `ι`, the RDF view `R(G)` is the set of quads,
each in the named graph of the element it comes from, generated as follows; `{base}` is the namespace's vocabulary base
and `lit(v)` the literal of `v` under `SCHEMA.md` section 5.1's datatype.

| From | Quads |
|---|---|
| An element `e` of label `l` with `ι(e)` defined | `ι(e) rdf:type C` for `l` and each of its supertypes and statement types |
| A primitive field `a` with value `v` | `ι(e) {base}a lit(v)`; none when an optional field is absent |
| A `set` field | One quad per member |
| A `list` or `rec` field | One quad whose object is an `rdf:JSON` literal |
| An interval field | `{a}_start` and `{a}_end` for its present bounds, and a bound's precision where coarser than `T`'s (`SCHEMA.md` section 5.1) |
| A measure with its unit on the value | One `ugdb:measure` literal |
| A catalog type as a value | The class IRI, as OWL 2 punning allows |
| `asserted` | `ugdb:asserted_start`, `ugdb:asserted_end` and `ugdb:live` |
| An element of a node-encoded relation `R` | `ι(e) {base}R_r ι(p)` for each role field `r` holding `p` |
| An element of an edge-encoded relation `R` | `ι(anchor) {base}nav ι(other)`, `nav` the anchor role's navigation name; with a reifier, also `ι(e) rdf:reifies <<( ι(anchor) {base}nav ι(other) )>>` and `e`'s attribute quads on `ι(e)`; for a `@symmetric` relation, the quad in both directions, as the view answers both |
| An element of a role label `R_r` | `ι(instance) {base}R_r ι(participant)` |
| An element of `Undeclared` | `ι(subject) predicate object` |

The view's derived properties, an inverse navigation name and a node-encoded relation's navigation property, and a
super role's triple are answered by the query core from these quads (`SCHEMA.md` section 11.2) and are no part of
`R(G)`.

**Proposition 17.2 (monotone; here).** For an injective homomorphism `h : G → G'` that preserves names,
`R(G) ⊆ R(G')`. So `R` is a functor from valid APGs on `S(K)` with such homomorphisms to datasets ordered by inclusion,
and adding elements only adds quads, which the monotonic reads of the RDF view rest on. *Proof.* Every quad of
definition 17.1 is generated by one element from its value and the names of the elements it references, which `h`
preserves.

**Proposition 17.3 (round trip; here).** On valid APGs on `S(K)`, `R` is injective up to naming: a partial function
`R⁻¹` on `R`'s image gives `R⁻¹(R(G)) = G`, each element identified by its IRI, its triple or its pair (section 15.1).
*Proof.* The element named by an IRI is recovered with its label from its type quads, as the most specific of them,
unique because an element's types form one chain of supertypes. Its fields are recovered from the quads under its IRI: a
label's fields have distinct names, so distinct predicates; an optional field's absence is a missing quad, and a present bound's missing precision is its type's own; a set's
members are its quads, without duplicates; a list or record is one literal. An element of an edge-encoded relation is
recovered from its reifier where it has one, else from its triple, unique by section 15.1, and a `@symmetric` relation's
two quads give one element. A role label's elements are its role quads; a role quad belongs to a set-valued role, so to
a role label, exactly when the relation's declaration makes that role set-valued.

**Corollary 17.4.** The SHACL shapes `SCHEMA.md` section 11.2 publishes hold on `R(G)` for every valid `G`, since each
shape restates a typing rule or a state constraint of section 14.2 over the quads of definition 17.1.

## 18. The GQL view

**Definition 18.1 (`P`).** For a valid APG `G` on `S(K)`, the GQL view `P(G)` is the property graph of PG-Schema's
definition 3.1 and MGQL's `PropertyGraph`, with:

| Part of `P(G)` | From |
|---|---|
| A node | Each element of an entity label or `Resource`; each element of a node-encoded relation label, as its relation node; each element of an edge-encoded relation label whose type allocates a reifier, as its relation node, beside its edge (`SCHEMA.md` section 11.3) |
| A stored edge | Each element of an edge-encoded relation label, from its anchor's node to its other participant's, undirected when the relation is `@symmetric`; each role field of a node-encoded relation's element and each element of a role label, from the relation node to the participant's node, with the role's name and, for a specialized role, the super role's |
| A derived, read-only edge | The two role edges of a relation node of an edge-encoded relation; a binary node-encoded relation's navigation edge, from the participant gaining the name to the one it reaches |
| Labels | For a node or edge from an element of label `l`, the exact set of `l`, its supertypes and the statement types covering it |
| Properties | Each primitive field; a `list` or `set` field as `LIST`, a record, interval or measure as `RECORD`; `type`, the label's name; `asserted` and `live` on a `@bitemporal` element; an absent optional field absent |
| Element id | `ι` of the element; none for a derived edge, a role edge, or an edge without a reifier |

The **graph type** `Γ(S(K))` is closed: a node type per label presented as a node, with the exact label set of the
label and its supertypes and the property types of its primitive fields; an edge type per edge-encoded relation, per
role edge and per navigation edge, its endpoints a triple for each pair of concrete labels its roles admit.

**Proposition 18.2 (conformance; here).** For every valid APG `G` on `S(K)`, `P(G)` conforms to `Γ(S(K))` under
PG-Schema's strict, closed semantics (PG-Schema, definition 4.2), and so to the closed graph schema MGQL's
`graphConformsSchema` checks. *Proof.* A node or edge from an element of label `l` carries exactly the label set of
`l`'s node or edge type, and the properties of `σ(l)`'s primitive fields, of the types `Γ` lists. An edge's endpoints
are elements whose labels its role's type admits, by the typing of the field it comes from, and `Γ` lists a triple for
each admitted pair; derived edges are built from fields in the same way.

**Proposition 18.3 (monotone; here).** For an injective homomorphism that preserves names, `P(G)` is a subgraph of
`P(G')`. *Proof.* As proposition 17.2.

**Proposition 18.4 (one graph, two views; here).** On valid APGs, `P(G) = P(R⁻¹(R(G)))`: the GQL view is determined by
the RDF view. A GQL node corresponds to the IRI of its element, a stored edge to its reifier's IRI or its triple, and a
property to the literal of the same field, the correspondence `../query/QUERY-SPARQL-CORE.md` section 12 states for the
differential tests. *Proof.* Proposition 17.3.

## 19. The test oracle

Definitions 17.1 and 18.1 are the mapping contract between the RDF view and the GQL view, and propositions 17.3 and
18.2 its properties. The oracle checks the engine against them:

1. **Generate.** Valid APGs on a schema, the Gen5 schema and the conformance corpus's among them, are generated by the
   Lean model's executable named layer (section 20), respecting the constraints of section 14.2.
2. **Write.** Each APG is written through the engine, by SPARQL updates, GQL inserts or UQL mutations in turn.
3. **Read and compare.** The engine's RDF view, every quad of the dataset, must equal `R(G)` up to a bijection between
   `R(G)`'s blank nodes and the IRIs the engine minted for them, and `R⁻¹` of it must equal `G`. `R(G)` names a keyed
   element by the IRI its key mints (`SCHEMA.md` section 7.3) and an element whose IRI the engine draws, a handle or a
   reifier, by a blank node, as a write names it (`../query/QUERY-SPARQL-CORE.md` section 8.1); the engine's GQL view, every node and edge with labels and properties, must equal `P(G)` up to element ids that
   `ι` fixes, and must conform to `Γ(S(K))` under MGQL's checker.
4. **Query.** Queries in the overlap of the two cores run on the engine and on the reference evaluators, spareval over
   `R(G)` and MGQL over `P(G)`, and their answers are compared under the correspondence of proposition 18.4.

A failure names the side that diverged: the engine's write path, one view, or a query core.

## 20. The Lean model

Decided 2026-11-08: the Lean model of this file sits in `../../lean/schema`, a library of the package that holds
`PlacementModel` and `RoundModel`, under the repository's toolchain, Lean 4.14, without Mathlib, and takes a binary core
with a named layer over it.

A spike of 2026-11-08 under Lean 4.14 measured the two forms of definition 3.2:

| Under Lean 4.14 | Binary form, `0`, `1`, `+`, `×` | Named form, records and variants over field lists |
|---|---|---|
| The type declares | As a plain inductive type | As a type nested through `List (Name × Ty)` |
| Recursion over types | Structural | Mutual, over a type and its field lists |
| `V_E` computes by `rfl` | Yes | Yes |
| `deriving DecidableEq` | Yes | Unimplemented for nested types; written by hand |
| One record, one term | By construction | Only under an invariant: two orders of one record's fields are two terms |

The forms trade as follows:

| | Named form alone | Binary form alone | Binary core, named layer |
|---|---|---|---|
| Fidelity to definition 3.2 and to UGS | Direct | Fields become positions | Direct, at the layer part 3 reads |
| Field names for RDF predicates and GQL property keys | Present | A side table, a named layer under another name | Present |
| Equality and decidability | By hand, with a field-order invariant in every proof | Derived; structural equality | Derived on the core; by hand on the layer alone |
| Proofs of functoriality, `Δ` and proposition 8.4 | Mutual induction over field lists throughout | Structural induction | Structural, on the core |
| Executable as a test oracle, as MGQL is | Yes | Yes | Yes |
| Added work | List machinery in every theorem | Names rebuilt when part 3 needs them | Proposition 3.3 proved once |

The model takes a **binary core with a named layer over it**:

- The core states the paper's definitions in its binary form and proves sections 5 to 9 by structural induction. The
  paper references a Coq development of its results (paper §7), which this file has not read; the core's statements
  are to be compared against it where it exists.
- The named layer holds the types of definition 3.2, with a record's or variant's field names strictly increasing in
  the total order proposition 3.3 fixes, so one finite map is one term, and its decidable equality written by hand.
- Proposition 3.3 is proved between them, values and terms both, which carries every core result to the named layer.
  Part 3 reads the named layer, where each field's name gives its predicate and property key.
- A view that needs a declaration order keeps it beside the type, outside the record.
- Part 2's `set t` takes, without Mathlib's finite sets, a list without duplicates in the order of a decidable order on
  values, so one set is one term, as one record is.
- Part 3's views, `R` and `P`, are executable functions on the named layer, which section 19's oracle runs.

The translation theorem is the model's one substantial proof: it must hold naturally in `E`, and for terms as well as
values, or the core's results do not transfer.

## 21. Decisions

| Question | Decision |
|---|---|
| Foundation of UGS's semantics | The APG model of this file, decided 2026-11-08 after the comparison with PG-Schema (section 10) |
| GQL view | A closed graph type with PG-Schema's STRICT, closed semantics, one node type per concrete type with its exact label set (section 10) |
| Constraint semantics | PG-Keys for keys, uniqueness, presence and single values; PG-Schema's proposed `COUNT` for participation bounds; UGS's own for the rest (section 10) |
| RDF view | RDF 1.2 with reifiers and SHACL shapes, cross-checked against the common core of SHACL, ShEx and PG-Schema (section 10) |
| Distributivity | `C_S` is distributive (section 8.2) |
| Lean model | In `../../lean/schema`, a binary core with a named layer, proposition 3.3 proved between them (section 20) |
| Collections | Decided 2026-11-08: collections of values as `list t` and `set t` over label-free types; set-valued roles reified as role labels; a symmetric relation read up to swap (section 12) |
| `map` and `filter` | Decided 2026-11-08: over collections of values, with a body that reads the member alone (definition 12.4) |
| Finite images | Decided 2026-11-08: a label's image holds no `list`, and every primitive type in it has finitely many values (section 9) |
| Extended mapping terms | Decided 2026-11-08: a mapping's terms may read its valid source, by key lookups, inverse navigations, derivations, folds and functions; such a mapping is no functor, and its report says so (definition 16.3) |
| Colimits | Decided 2026-11-08: coproducts alone; no pushout or coequalizer, which would identify elements (definition 16.5) |
| Unnesting | Decided 2026-11-08: a collection's members as elements of a derived label, one origin each, for migrations that turn members into instances (definition 12.7) |
| History through a migration | Decided 2026-11-08: a target's history before its cutover is the migration evaluated over the source's believed slices, named as the present is (proposition 16.6) |
| `restrict open` | Decided 2026-11-08: refuses a retraction while open instances name the participant, and retracts the live instances that are no longer open with it (`SCHEMA.md` section 8.4, rule 6) |
| Participants of `@bitemporal` relations | Decided 2026-11-08: `@bitemporal` or `@append_only` types alone (`UGS0602`), so the live and believed slices are closed under references (proposition 15.2) |
| Equality of mapping terms | Decided 2026-11-08: no decision procedure until a feature needs one, since UGS declares no path equations and type-checking a mapping needs no equality; normal forms decide the fragment without variants, and two mappings are compared by evaluation on the generated APGs of section 19 |
| A catalog's schema | Decided 2026-11-08: one label per concrete type and role label; a supertype as the variant of its concrete subtypes; navigation names as inverse references, never fields (section 13) |
| Constraints | Decided 2026-11-08: state constraints on one APG and transition constraints on a commit, each held when its violation query returns nothing (section 14) |
| History | Decided 2026-11-08: a state per commit, read through its live, believed or full slice, closed under references (section 15.3) |
| The views | Decided 2026-11-08: `R` and `P` of definitions 17.1 and 18.1 are the mapping contract; `R` round-trips, and `P` conforms to the generated graph type (sections 17 to 19) |

## 22. Open questions

None at present. The questions of 2026-11-08 are decided in section 21; a feature that needs equality of mapping terms
decided reopens the choice of procedure recorded there.

## 23. Sources

- J. Shinavier, R. Wisnesky, J. G. Meyers, *Algebraic Property Graphs*, arXiv:1909.04881v3, 2022: the schema and the
  APG (§3), the single-set form (§3.1), the element kinds (§4.2), coalgebras and models (§5), user-defined functions
  and further types (§6), schema mappings and `Δ` (§7.1), polynomial functors (§8). Its Coq proofs are referenced at
  `http://categoricaldata.net/APG.v`.
- J. R. B. Cockett, R. A. G. Seely, *Finite Sum-Product Logic*, Theory and Applications of Categories 8(5), 2001: the
  free category with finite sums and products.
- G. Scherer, *Deciding Equivalence with Sums and the Empty Type*, POPL 2017.
- D. I. Spivak, *Functorial Data Migration*, arXiv:1009.1166: `Δ`, `Σ` and `Π` over schemas presented as categories.
- T. Grust, *Monad Comprehensions: A Versatile Representation for Queries*, 2003: the collection types part 2 draws on
  (paper §6.2).
- CQL, `src/catdata/apg`: the named records and variants of definition 3.2, the transform conditions of definition
  7.1, and the per-label type and term of definition 9.1.
- R. Angles et al., *PG-Schema: Schemas for Property Graphs*, SIGMOD 2023 (PACMMOD 1(2)), arXiv:2211.10962: the
  property graph (definition 3.1), formal graph types and conformance (section 4.2), inheritance (section 4.1).
- R. Angles et al., *PG-Keys: Keys for Property Graphs*, SIGMOD 2021: `EXCLUSIVE`, `MANDATORY`, `SINGLETON` and
  `IDENTIFIER`, and validation as the emptiness of a query (section 4.5).
- S. Ahmetaj et al., *Common Foundations for SHACL, ShEx, and PG-Schema*, arXiv:2502.01295, 2025.
- A. Thimmaiah, T.-N. Lin, M. Gligoric, *MGQL: An Executable, Small-Step Semantics of GQL*, OOPSLA 2026: the closed
  graph schemas the GQL view's conformance is checked against.
