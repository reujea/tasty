# ADR-0302: A user file open selects its result tab — amends the focus clause of ADR-0279

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: file-handler, focus, explorer, user-action, adr-0279
- **Group**: file-handler

## Context

[ADR-0279](0279-file-dispatch-retains-origin-through-completion.md) decided that file
dispatch keeps an explicit origin through asynchronous completion, and included one
clause about focus: `OpenSurface creates a tab in the origin's pane without changing
focus`. Its Context is the asynchronous routing problem — identification finishes later,
a window other than the requester may be focused by then, and selecting the focused
window at completion sends the result to another engine. Every sentence of that Context
is about `file_handler.dispatch`, the agent IPC method, and the focus policy section
that records it lists only IPC methods.

The implementing commit changed `open_surface_tab` alone and branched on whether an
origin was supplied. Origin is a routing value, not a statement about who asked, so the
clause reached one caller its Context never discussed: the explorer double click, which
had been supplying its own surface as the origin since the explorer was introduced. That
commit touched no explorer code and named it in no message, document or test.

The observable result is that the same user action behaves differently depending on
where it was issued. Opening a file from the terminal link, a drop or the file picker
selects the new tab; opening it by double clicking in the explorer does not, because
only the explorer supplies an origin. The user has to click the tab that just appeared.

The origin cannot be repaired by reading the intent's own origin label. File
identification crosses a worker thread, and the event that returns carries named fields
rather than the dispatching intent, so `IntentOrigin` does not survive the round trip.
The transport is not a usable left side either: the markdown plugin turns a click on a
link inside a document into a `file_handler.dispatch` call, so a user action can arrive
through the agent method.

## Decision

**A file open that the user performed directly selects its result tab; an agent request
does not.** Dispatch carries a `FileDispatchOrigin` (`User` or `Agent`) beside the
existing origin surface, through the identification round trip and the handler picker,
and `open_surface_tab` restores the previous selection only for `Agent`. Emitters state
the value: the explorer double click, the terminal link click, the file drop, the file
picker confirmation and the terminal link context menu are `User`; `file_handler.dispatch`
is `Agent`, because the request carries nothing that would let the host tell an agent
apart from a plugin relaying a click. The same value also labels the intent that the
no-origin branch dispatches, which until now announced every caller as a user menu.

**What this does not amend.** ADR-0279 stays in force for everything else: an explicit
origin is still the request target, the initial request and the completion still route to
the engine owning that surface including parked engines, the origin is still retained
through picker selection, a disappeared origin still executes nothing and never becomes a
focused NewTab, and a request with no origin still keeps its existing focused-window
behaviour. Routing and focus are separate axes and only the second one changes.

## Consequences

- **얻은 것**: the four user paths that open a file now agree with each other, and the
  agent path keeps the guarantee ADR-0279 bought. The user/agent split is one value, so
  effects that diverge derive from it rather than from separate flags.
- **잃은 것**: every `DispatchFile` emitter must now state its origin, and a new emitter
  that forgets to think about it will pick whatever its author typed.
- **운영 비용 / 유지 부담**: the value rides the same carry path as `origin_surface_id`
  and `ignore_size_limit`, so a new hop in that path has to forward one more field.

## Alternatives Considered

- **Branch on `IntentOrigin` at the dispatch site**: the label does not cross the
  identification worker, and it would misclassify the markdown link click, which is a
  user action arriving through the agent IPC method.
- **Let the explorer pass no origin**: one line, but it discards the routing half too.
  The result would land in whichever pane holds focus rather than the explorer's own,
  which is the behaviour the focus policy exists to prevent.
- **Decide by whether the origin's pane currently holds focus**: reintroduces the race
  ADR-0279 removed, and uses focus to decide an outcome, which the focus policy forbids.
- **Leave it and document the current behaviour as intended**: defensible only if the
  clause was aimed at the explorer, and the introducing commit shows it was not.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- A `DispatchFile` emitter appears that is neither a direct user gesture nor an agent
  request, so that the two variants no longer partition the callers.
- `IntentOrigin` gains a representation that survives the identification round trip:
  the separate value then duplicates an axis and should be collapsed into it.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- Plugins relay user gestures often enough that `file_handler.dispatch` needs to express
  the distinction itself, rather than being fixed to `Agent`. 재는 법: open a document
  in the markdown surface, click a link inside it, and observe with `tab.list` whether
  the tab that appears is active; today it is not, and that is the cost of the fixed
  value.

## References

- 개정 대상: [ADR-0279](0279-file-dispatch-retains-origin-through-completion.md) (focus clause)
- 부분 개정: [0526](0526-a-plugin-popup-the-user-touched-makes-its-file-dispatch-a-user-action.md) (`file_handler.dispatch` 를 `Agent` 로 고정한 조항 개정 — 사용자가 만진 plugin popup 을 실으면 `User`)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- [Focus policy](../design/policies/focus.md)
- [File handler](../features/file-handler/index.md)
- Current implementation: `core::origin::FileDispatchOrigin`, `open_surface_tab`,
  `file::dispatch::apply_identify_result`, `file::dispatch::apply_file_picker_result`.
