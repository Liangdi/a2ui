# Opt-in resumable expression evaluation (prototype)

`MessageProcessor::start_evaluation(surface_id, base_path, expression, mode)`
returns an opaque handle. `EvaluationMode::Reactive` subscribes to expression
results; `EvaluationMode::Action` is a fresh imperative invocation. Only a trusted
host user gesture may start an action evaluation. This API is additive: existing
synchronous `DataContext` consumers are unchanged.

Drain `drain_evaluation_updates()` for `Ready(Value)`, `Pending`, `Cancelled`, or
`Error { code, message }`. Cancellation is a lifecycle state, not an expression error. Updates coalesce by handle until drained. Non-fatal
template diagnostics are separately available through `drain_evaluation_diagnostics()`
and do not erase a successfully rendered partial string. Repeated identical
diagnostics are not re-emitted on unrelated replies. Drain the
ordinary outgoing message queue for `callAgentFunction` messages. Feed incoming
`agentFunctionResponse` messages through `process_message`; matching responses
resume evaluations. Unknown, cancelled, duplicate and stale replies do not alter
active results. No implicit transport or network connection is created. Hosts without a delivery
path should call `set_agent_transport_available(false)`; new remote evaluations
then fail locally without queueing an RPC. It defaults to true for existing
queue-draining hosts. The flag does not cancel previously queued/transmitted calls.

Evaluation recursively resolves bindings, literal containers and function calls.
All sibling arguments are visited before returning `Pending`, allowing independent
RPCs to proceed in parallel. Native execution and remote fallback use the exact
catalog and policy checks in `prepare_renderer_call`. Resolved argument data is
not reinterpreted as expressions. Reserved system functions never become RPCs.

Reactive calls share in-flight/settled values by surface, catalog,
function and canonical resolved arguments. Action handles and expression nodes
have separate call identities. Completed native action nodes are memoized so an
async sibling's response cannot repeat a side effect. Action evaluations finish
and release their cache entries after a terminal result.

Data/component updates reevaluate retained expressions and release calls whose
resolved arguments changed. Unchanged calls retain shared pending/settled results.
An existing failed binding retries only after its arguments change; a new binding
may make a fresh identical request. Pending actions are **cancelled** on such updates rather than mixing
old and new argument state. Surface deletion cancels all handles and removes
unsent requests. Already-transmitted requests cannot be recalled, but late replies
are ignored. Explicit cancellation releases unused shared calls; another active
subscriber keeps its shared request alive. `cancel_surface_calls(surface_id)`
drops in-flight work while retaining subscriptions, suspended until the next
surface invalidation. Hosts that directly mutate a data model
must call `invalidate_evaluations(surface_id)`. Reset discards all evaluator state.

Hosts must cancel unused reactive handles and drain result/outgoing queues. Limits
are 1,024 active handles, 4,096 cached remote calls, 4,096 undrained handle updates,
and 128 nested expression levels. Capacity errors are explicit.

## Deliberate prototype boundaries

- Current `activate_component` actions use the resumable path automatically.
  Event context entries resolve sequentially in source order, then userMessage;
  failures stop the action. `start_check` offers passive subscriptions with pending
  checks non-failing until settled. Hosts drain `drain_check_updates()` and choose
  how to present validation. Automatic renderer property/check subscriptions and
  their visual states are not wired yet.
- Action errors appear both as outgoing protocol errors and in
  `drain_action_errors()` for local presentation; hosts should not double-report
  the two views of the same error.
- No timeout, transport retry, remote cancellation protocol, or persistence across
  processor reset. Host transport owns those decisions.
- Declared remote return types are enforced before results settle. Full return-schema
  constraints and component-level error boundaries remain to be integrated.
- `formatString` template expansion remains in the synchronous native function;
  nested remote calls inside template strings never route: their parts render empty
  and report an expression diagnostic, matching the protocol template boundary.
- Invalidating all expressions on a surface is conservative. Fine-grained dependency
  tracking and focus-preserving renderer reconciliation are future improvements.

The dedicated regression suite covers parallel nested RPCs, sharing/isolation,
stale reply rejection, cancellation, literal escaping/integer precision, remote
errors, and non-repetition of native action effects.
