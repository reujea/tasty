# ADR-0279: File dispatch retains its origin through completion

- **Status**: Accepted
- **Date**: 2026-09-15
- **Tags**: file-handler, focus, routing, lifecycle
- **Group**: file-handler

## Context

File dispatch accepts an optional origin surface. Each window owns an engine, while
identification completes asynchronously. Selecting the focused window at completion
can send the result to another engine. Treating a missing origin like an omitted one
then creates a tab in an unrelated pane. Handler pickers introduce a second delay.

## Decision

An explicit origin is the request target for `file_handler.dispatch`. Route the initial
request and IdentifyDone completion to the engine that owns that surface, including
parked engines. Keep the origin in picker data until selection. OpenSurface creates
a tab in the origin's pane without changing focus. Validate the origin again before
any handler action; a disappeared origin never becomes a focused NewTab request.

Initial unknown targets use the existing invalid-params response and unowned-target
message. The accepted response continues to mean queued, not completed. Failures after
acceptance use the existing asynchronous warning channel; they do not send a second
RPC response. Picker cancellation executes nothing and records no recent handler.
Requests with no origin keep their existing focused-window and user NewTab behavior.
Ipc handler payloads remain path-only; this decision does not redefine plugin methods.

## Consequences

- Focus changes during identification cannot redirect an explicitly targeted result.
- A closed origin stops completion, even when another window remains available.
- The accepted response alone does not report the eventual handler outcome.
- Picker data carries one additional optional surface ID. IDs remain globally unique.

## Alternatives Considered

- Route only the initial request: completion would still reselect the focused window.
- Fall back when the origin closes: creates an unrequested tab in a different pane.
- Add completion RPC responses: changes the existing asynchronous protocol contract.

## Reconsideration Triggers

**Repository-readable conditions**:

- Surface IDs become reusable or stop being globally unique: introduce a generation
  or request-scoped ownership token before relying on a retained numeric ID.
- File dispatch gains an asynchronous result protocol: expose post-acceptance failure
  through that protocol while preserving the no-fallback rule.

**Human-observed conditions**:

- Callers need a completion receipt: reproduce their workflow with a delayed detector
  and compare enqueue acknowledgment with the eventual handler result.

## References

- 부분 개정: [0302](0302-a-user-file-open-selects-its-result-tab.md) (focus clause 개정)
- [Focus policy](../design/policies/focus.md)
- [File handler](../features/file-handler/index.md)
- Current implementation: `request_resource_id`, `App::handle_identify_done`,
  `file::dispatch::apply_identify_result`, `file::dispatch::apply_file_picker_result`, `core::origin::require_origin_pane`.
