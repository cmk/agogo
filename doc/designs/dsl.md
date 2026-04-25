# Polyrhythm DSL (v0.3+)

## Design principle

A rich underlying mathematical model lets the surface DSL stay small.
The `Grid` lattice (`grid.md`) gives us a complete bi-Heyting algebra
of single-bar grids; the `i8` + `TBase` swing model adds one
per-track modifier. Together they cover the rhythmic universe agogo
targets, so the DSL is a thin syntactic shell.

## Grammar

```
poly       := track ( '&' track )*                    # polyrhythm (loosest)
track      := expr modifier*
expr       := impl_expr
impl_expr  := join_expr ( ('>' | '<') join_expr )*    # imply / coimply
join_expr  := meet_expr ( '|' meet_expr )*            # join / LCM
meet_expr  := unary ( '^' unary )*                    # meet / GCD
unary      := '!' unary | primary                     # negation (prefix)
primary    := atom | '(' expr ')'
atom       := 'T' <n>              # binary       (n ∈ {1,2,4,...,256})
            | 'T' <n> 't'          # triplet      (n ∈ {2,4,...,512})
            | 'T' <n> 'q'          # quintuplet   (n ∈ {2,4,...,512})
            | 'T' <n> 'p'          # 15-tuplet    (n ∈ {2,4,...,512})
modifier   := '~' <TBase> ':' <i8> # swing (resolution : amount)
            | '@' <ticks>          # musical offset (signed i32, in ticks)
            | '+' <ms>             # delay compensation (positive u32, in ms)
```

## Operators

### Precedence (tightest to loosest)

```
1. !          prefix negation (Heyting pseudo-complement)
2. ^          meet / GCD
3. |          join / LCM
4. >  <       implication / coimplication
5. &          polyrhythm separator
```

All binary operators are left-associative.

### Operator table

| Symbol | Name | Lattice semantics | Result |
|--------|------|-------------------|--------|
| `!` | negation | Heyting pseudo-complement: ¬a = a → ⊥ | single grid |
| `^` | common refinement | meet (gcd of tick counts) | single grid |
| `\|` | alignment intersection | join (lcm of tick counts) | single grid |
| `>` | implication | Heyting →: max{c : a ∧ c ⊑ b} | single grid |
| `<` | coimplication | co-Heyting \\: min{c : a ⊑ b ∨ c} | single grid |
| `&` | polyrhythm | parallel voices (not a lattice element) | multiple tracks |

The lattice's divisibility ordering is *inverted* relative to
event-set inclusion (finer grids divide coarser ones). The DSL
surface picks event-set semantics because users think in firings,
not divisors.

### Why `^` and `|` are dual

`a ^ b` refines two grids to their finest common subdivision — the
events on *either* grid, packed onto a uniform grid (gcd).

`a | b` intersects two grids to their common alignment — the events
on *both* grids, which form a uniform grid (lcm).

### Why `&` is not an algebraic operator

`a & b` keeps two grids as parallel voices. Their union of firings
is generally an irregular sequence that cannot be expressed as a
single grid. That irregularity is exactly what makes `&` the
polyrhythm marker rather than another algebraic operator: the
operations that close inside the lattice get algebra symbols,
the one that doesn't gets a separator.

### Heyting implication and coimplication

`a > b` (implication) answers: "what is the largest grid `c` such
that `meet(a, c) ⊑ b`?" — the largest subdivision compatible with
`a` that stays within `b`.

`a < b` (coimplication) answers: "what is the smallest grid `c` such
that `a ⊑ join(b, c)`?" — the smallest subdivision that, combined
with `b`, covers `a`.

`!a` (negation) is `a > ⊥` — the largest grid orthogonal to `a`.

These three operations complete the bi-Heyting algebra structure of
the Grid lattice. They exist mathematically on all 36 elements;
concrete musical use cases will guide which ones earn prominent
surface syntax over time.

## Modifiers

| Modifier | Syntax | Semantics |
|----------|--------|-----------|
| swing | `~T16:80` | `SwingConfig { resolution: T16, amount: 80 }` — explicit binary resolution + signed i8 tick offset |
| offset | `@-50` | Musical offset in ticks (signed i32). Tempo-dependent. |
| delay | `+5` | Hardware delay compensation in milliseconds (positive u32). Tempo-independent. |

Swing resolution is an explicit `TBase` argument (not inferred from
the grid). The binary axis (`Grid.n`) is the structural backbone of
the lattice; the `t`/`q` flags are orthogonal. Any `TBase` can serve
as swing resolution regardless of the grid's track.

Modifiers bind to the preceding track expression. Each modifier kind
may appear at most once per track; duplicates are a parse error.

## Examples

```
T16~T16:80                     # 16ths with +80-tick swing at T16 resolution
T4 & T2q                       # classic 4-against-5: quarters vs quintuplet halves
T16t & T16q                    # 24-vs-40 polyrhythm at the 16th-note level
T8 | T8t                       # alignment intersection of 8ths and triplet 8ths (= T4)
T16 ^ T16t                     # common refinement of 16ths and triplet 16ths (= T32T)
T16~T16:80 & T8t@-50           # swung 16ths against triplet 8ths offset back 50 ticks
T8 & (T16t ^ T16q)             # 8ths in parallel with the 15-tuplet refinement
!T16                           # pseudo-complement of T16
T16 > T8                       # Heyting implication
T16 < T8                       # co-Heyting coimplication
```

## Deferred

- **Multi-bar phrases**. Excluded by the lattice's square-free
  constraint (`grid.md` §V). The DSL is single-bar
  only by construction.
- **`coneg` (co-Heyting co-negation) in the DSL surface.** `coneg(x) =
  ⊤.coimp(x)` exists via the `Coheyting` trait but has no dedicated
  syntax yet — compose via `T1 < x`. Add a prefix operator only
  when a concrete use case names itself.
- **`comid` (co-Heyting boundary/co-middle).** `comid(x) =
  x.meet(x.coneg())` exists via the `Coheyting` trait. No DSL
  surface syntax yet.

## Why this stays small

- Atoms are grid names — given for free by the lattice.
- Algebraic operators are meet, join, imply, coimply, neg — given
  for free by the bi-Heyting structure.
- Polyrhythm is event-set union — given for free by the fact that
  union doesn't close inside the lattice.
- One swing modifier — given for free by the i8/TBase model.
- Offset and delay — one musical, one physical, orthogonal units.

Every piece of grammar maps to one piece of underlying math.
Adding pattern combinators on top would re-invent expressivity
the lattice already provides; we resist it.
