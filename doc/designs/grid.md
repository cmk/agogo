$$ \text{Formal Theory: The Square-Free } \{2, 3, 5\} \text{ Grid Subdivision Lattice } \mathcal{L} $$

$$ \begin{array}{l}
\text{I. The Set of Grid Subdivisions} \\
\mathcal{L} = \{ \text{bar} \cdot 2^{-n} \cdot 3^{-i} \cdot 5^{-j} \mid n \in [0, 8], i,j \in \{0, 1\} \} \\
\text{where } \text{bar} = 4 \cdot \text{PPQN} = 3840 \text{ ticks at PPQN}=960. \\
\text{Cardinality: } 9 \times 2 \times 2 = 36 \text{ (the divisors of } 3840 = 2^8 \cdot 3 \cdot 5\text{).} \\
\\
\text{II. Lattice Properties (Heyting Algebra)} \\
\text{The lattice is bounded by:} \\
\bullet \text{ Top Element } (\top): \text{bar} = 3840 \text{ ticks (the whole note, } T_1\text{)} \\
\bullet \text{ Bottom Element } (\bot): 1 \text{ tick (} T_{512p}\text{)} \\
\\
\text{The structure is a Heyting Algebra because for every } a, b \in \mathcal{L}, \text{ there exists a unique } a \to b: \\
\text{Implication: } a \to b = \text{max} \{ c \in \mathcal{L} \mid c \wedge a \sqsubseteq b \} \\
\text{Pseudo-complement: } \neg a = (a \to \bot) \text{ (The "remainder" or smallest resolvable gap).} \\
\\
\text{III. Mapping Tracks to Coordinates } (n, i, j) \\
\begin{array}{|l|l|l|l|}
\hline
\text{Track} & \text{Tick formula} & \text{Coordinate} & \text{Plan variants } (n \in [0, 8]) \\
\hline
\text{Binary} & \text{bar} / 2^n & (n, 0, 0) & T_1, T_2, T_4, \ldots, T_{256} \\
\text{Triplet} & \text{bar} / (3 \cdot 2^n) & (n, 1, 0) & T_{2t}, T_{4t}, \ldots, T_{512t} \\
\text{Quintuplet} & \text{bar} / (5 \cdot 2^n) & (n, 0, 1) & T_{2q}, T_{4q}, \ldots, T_{512q} \\
\text{15-tuplet} & \text{bar} / (15 \cdot 2^n) & (n, 1, 1) & T_{2p}, T_{4p}, \ldots, T_{512p} \\
\hline
\end{array} \\
\\
\text{IV. Algebraic Sanity Checks} \\
\bullet \text{ Square-Free Constraint: } i,j \le 1 \text{ ensures that } \text{triplet}(\text{triplet}(x)) \text{ is outside the system.} \\
\bullet \text{ Distributivity: } a \wedge (b \vee c) = (a \wedge b) \vee (a \wedge c). \text{ Common grids and polyrhythms} \\
\text{interact predictably without jitter or rounding error until the } \bot \text{ threshold is hit.} \\
\\
\text{V. Musical Limitations} \\
\text{By capping the lattice at the bar (} n \ge 0 \text{):} \\
1. \text{ The system is a single-bar "Clock Divider."} \\
2. \text{ Multi-bar phrases cannot be expressed (no upward extension above } T_1\text{).} \\
3. \text{ The Bar is the universal reference point (} \top \text{, the lattice top).}
\end{array} $$

