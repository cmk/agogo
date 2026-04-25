# Polyrhythm DSL (v0.3+)

## Design principle

A rich underlying mathematical model lets the surface DSL stay small.
The `Grid` lattice (`grid.md`) gives us a complete algebra of
single-bar grids; the `i8` + `TBase` swing model adds one
per-track modifier. Together they cover the rhythmic universe agogo
targets, so the DSL is a thin syntactic shell.

## Grammar

```
poly       := track ( '|' track )*
track      := expr modifier*
expr       := atom
            | expr '&' expr        # alignment intersection (lattice join / lcm)
            | expr '^' expr        # common refinement     (lattice meet / gcd)
            | '(' expr ')'
atom       := 'T' <n>              # binary       (n ∈ {1,2,4,...,256})
            | 'T' <n> 't'          # triplet      (n ∈ {2,4,...,512})
            | 'T' <n> 'q'          # quintuplet   (n ∈ {2,4,...,512})
            | 'T' <n> 'p'          # 15-tuplet    (n ∈ {2,4,...,512})
modifier   := '~' <i8>             # swing amount (in ticks, signed)
            | '+' <ticks>          # shift  (Micro forward)
            | '@' <ticks>          # offset (Micro signed)
```

Precedence: modifiers bind to atoms; `&` and `^` are left-associative
at equal precedence; `|` has the lowest precedence and separates
parallel tracks. Use `(...)` to override.

## Operators

| Symbol | Name | Event-set semantics | Lattice semantics | Result |
|--------|------|---------------------|-------------------|--------|
| `&` | alignment intersection | firings on **both** grids | join (lcm of tick counts) | single track |
| `^` | common refinement | firings on either, packed onto a uniform grid | meet (gcd of tick counts) | single track |
| `\|` | polyrhythm | firings on **either** grid, kept as separate voices | — (not a lattice element) | multiple tracks |

The lattice's divisibility ordering is *inverted* relative to
event-set inclusion (finer grids divide coarser ones), which is
why `&` on event-sets equals `∨` on the lattice and `^` equals
`∧`. The DSL surface picks event-set semantics because users
think in firings, not divisors.

## Why `&` and `|` are dual but distinct

`a & b` collapses two grids to a single grid — the events
common to both, which form a uniform grid (specifically `lcm(a, b)`).

`a | b` keeps two grids as parallel voices. Their union of firings
is generally an irregular sequence that cannot be expressed as a
single grid. That irregularity is exactly what makes `|` the
polyrhythm marker rather than another algebraic operator: the
operations that close inside the lattice get algebra symbols,
the one that doesn't gets a separator.

## Examples

```
T16~80                    # 16ths with +80-tick swing (66.6% triplet feel)
T4 | T2q                  # classic 4-against-5: quarters vs quintuplet halves
T16t | T16q               # 24-vs-40 polyrhythm at the 16th-note level
T8 & T8t                  # downbeats common to 8ths and triplet 8ths (= T4)
T16 ^ T16t                # finest grid containing both 16ths and triplet 16ths (= T32t)
T16~80 | T8t @ -50        # swung 16ths against triplet 8ths offset back 50 ticks
T8 | (T16t ^ T16q)        # 8ths in parallel with the 15-tuplet refinement of triplet & quintuplet 16ths
```

## Deferred

- **Heyting implication** `a → b` and pseudo-complement `¬a`.
  Both exist in the algebra (`grid.md` §II) but no
  concrete musical use case has named itself yet. Add to the
  surface only when one does.
- **Multi-bar phrases**. Excluded by the lattice's square-free
  constraint (`grid.md` §V). The DSL is single-bar
  only by construction.

## Why this stays small

- Atoms are grid names — given for free by the lattice.
- Algebraic operators are meet, join — given for free by the
  Heyting structure.
- Polyrhythm is event-set union — given for free by the fact
  that union doesn't close inside the lattice.
- One swing modifier — given for free by the i8/TBase model.

Every piece of grammar maps to one piece of underlying math.
Adding pattern combinators on top would re-invent expressivity
the lattice already provides; we resist it.
