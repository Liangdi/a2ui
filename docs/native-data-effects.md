# Native data effects

Trusted native `FunctionImplementation`s may implement the additive `data_effects`
method. Existing `execute` implementations and wire declarations are unchanged.
`data_effects` is pure preparation: it must not perform external side effects.
It returns at most 1,024 `DataEffect::Set { path, value }` commands. Paths follow
binding resolution: relative paths use the current template base, absolute paths
target the current surface. No command can select another surface.

The renderer action processor requires trusted user activation, applies the
entire batch to a private model snapshot, and rejects invalid writes before
calling `execute`. Only successful execution with a valid return type commits
that snapshot. The immutable model borrow is released before the atomic commit.
A failed batch or execution leaves the live model unchanged. Ordinary functions
returning no commands do not copy the surface model.

Imperative array/object/argument expressions run in wire order and stop at the
first pending or failed step. Completed nodes are memoized while a remote result
is pending, so resume cannot repeat committed native writes. Cancellation stops
remaining work but does not roll back earlier successful steps. Reactive
subscriptions are refreshed after commits. Reactive evaluation, synchronous
`DataContext` calls and incoming renderer RPC reject nonempty data effects.

This API controls model writes only. The library cannot roll back arbitrary
external effects performed inside a host's native `execute` implementation.
Hosts must keep such operations separately authorized and idempotent.

## Defensive formatting bounds

`formatNumber` and `formatCurrency` accept integral `decimals` from 0 through 100.
Negative, fractional, nonnumeric or larger precisions are rejected before string
allocation. This is a renderer safety limit, not a claim of unlimited precision.
