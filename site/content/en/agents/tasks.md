<!-- source-hash: d52404a7c85c -->
<a id="task-dag"></a>

# Task workflows (DAG)

Register tasks and connect their dependencies to run steps in the required order. Start with one task using `task-run`, `task-create`, and `task-await`, then add ordering and failure handling.

Create and manage tasks with the `tasty` CLI. If you are new to it, follow [Starting the runner](#starting-the-runner), [Creating a task](#creating-a-task) and [Checking the result](#checking-the-result) to run one task end to end with `task-run`, `task-create` and `task-await`. Read the later sections on ordering, passing results, branching and synchronisation when you need them. To follow them in the app, see [Watching progress](#watching-progress).

Pass the task's workspace as `--workspace` in each command (the older `--workspace-id` is still accepted). The command uses that workspace even while you are viewing another one.

## Starting the runner

You can create tasks, but nothing runs while the runner is off. Start it once per Workspace.

```sh
tasty agent task-run --workspace 2 --action start
tasty agent task-run --workspace 2 --action status
tasty agent task-run --workspace 2 --action stop
```

Restarting Tasty does not start the runner again automatically. The tasks themselves are still there, so `start` again and it picks up where it left off. A task that was waiting for an external completion signal before the restart is not finished by that signal afterwards; once its wait time runs out, a running runner ends it as a timeout failure. When the runner is stopped, the runner pill on the DAG screen says so, and hovering it shows the command that starts it again. In a DAG tab that command is also spelled out on the header line.

## Creating a task

```sh
BUILD=$(tasty agent task-create --workspace 2 --name build \
  --command '{"kind":"run","command":["cargo","build"]}' | jq -r .id)
```

Creating one returns a task ID. Use that ID to define dependencies and to query state.

| Command kind | What it does |
|---|---|
| `run` | Runs the specified command. It is a background process that does not occupy a terminal, and it carries up to the last 64KiB each of standard output and standard error in the result. Interactive programs do not fit here. It gets the environment Tasty was started from, with the exceptions below the table |
| `custom` | Turns one of Tasty's own actions into a task. Things that create a terminal, such as spawning a child agent, belong here. When the action is waited on separately until it finishes, a task sent in a graph keeps the first acceptance reply in the result's `raw.accepted`, apart from the final reply |
| `reduce` | Merges the results of several tasks into one |
| `wait_barrier` | Waits until all the signals have gathered at a barrier |
| `agent` | Sends one instruction to an agent session such as Claude or Codex and takes its answer as the result. See [Asking an agent and taking its answer as the result](#asking-an-agent-and-taking-its-answer-as-the-result) below |

You can also pull the command JSON out into a file and pass it as `--command @build.json`.

For a `custom` task that checks completion periodically, set `poll.interval_ms` to choose the interval (500ms by default). The first check is immediate; later checks wait that long after the previous request finishes. Tasks waiting on the same target check separately. A busy runner may check later, but does not issue missed checks in a burst. If you set `poll.timeout_ms`, the runner checks the deadline even before the next request is due. Starting the runner again after a restart resumes checking with the saved interval and deadline, without sending the original execution request again. An interval of 0ms allows one check each time the runner advances.

A single task definition (command, metadata, result types and so on) can be up to 768 KiB as JSON (less if the memory entry size limit is set below 1 MiB). A larger one is refused with an error and no task is created. Keep large data in a file and pass its path. If the outputs of earlier tasks filled in just before a run would push a task past this size, that task fails without running.

A `run` command, the postprocess command below and a reducer that merges results with a shell command get the environment Tasty was started from. If you started Tasty inside Claude Code, the variables that point at that session are not passed on (the same as in the terminal). If you started Tasty from another Tasty's terminal, `TASTY_SESSION_TOKEN`, `TASTY_SURFACE_ID`, `TASTY_PARENT_HOME` and `TASTY_AGENT_ID`, which point at that Tasty, are not passed on either, so a `tasty` call inside the command reaches this Tasty.

## Checking the result

```sh
tasty agent task-await --workspace 2 --id "$BUILD"   # wait until it finishes (up to 10 minutes by default)
tasty agent task-get --workspace 2 --id "$BUILD"     # look at the state and result again
```

When `outcome` in the `task-await` reply is `terminal`, the task has finished; whether it succeeded is in `state`, `succeeded` or `failed`. `timed_out` (the wait ran out) and `not_found` (no such task) also exit with status 0, so do not judge by the exit status alone. The exit code and output of a `run` task are in the reply's `result`. Wait-time options are in [Waiting and fixing up](#waiting-and-fixing-up).

That is the basic flow for running one task and checking its result. The sections below are the advanced steps: chaining several tasks in order, passing results, and branching.

## Order and failure handling

```sh
TEST=$(tasty agent task-create --workspace 2 --name test \
  --command '{"kind":"run","command":["cargo","test"]}' \
  --depends-on "$BUILD" --on-failure abort | jq -r .id)
```

Every task listed in `--depends-on` has to finish before this task becomes ready. A graph that would form a cycle is rejected at creation time.

| Failure policy | Meaning | Where you attach it |
|---|---|---|
| `abort` (default) | If a task it depends on fails, this task is skipped. Everything below it is skipped too | The depending side |
| `continue_downstream` | This task runs even if a task it depends on failed | The depending side |
| `fallback:<task ID>` | If this task fails, that task is woken up instead | The side that can fail |

It is easy to get the placement wrong. `abort` and `continue_downstream` have to be attached to the **following** task for them to count toward that task's readiness decision, while `fallback` has to go the other way, on the task that **can fail** itself. Using a policy on the wrong task will not give you the intended failure handling.

If Tasty is too busy to answer a `custom` task's request in time, the task does not fail right away. A late status check is asked again on the next check, and a request that expired before it ran is sent again. A request that had already started running (such as starting a child agent) is not sent again, so it does not run twice; Tasty waits for its late answer instead. If that answer never arrives (for example, the app restarted in the meantime), the task becomes `unknown` rather than failed. Check whether the request took effect (did the child start?) and then retry or cancel it.

A task used as a fallback has to be created before the main task. To stop the runner from running it first in the meantime, create the fallback with `--reserved-for-fallback`. The reservation prevents it from running before the main task is created. Once linked, it continues waiting until the main task fails and activates the fallback.

## Passing results from earlier tasks

`--depends-on` only ties order together. To pass an earlier task's result as a later task's **input**, use a placeholder.

```
${task.<task ID>.output}          the whole result
${task.<task ID>.output/stdout/text}   one value inside the result
```

The real value goes in that spot when the task is dispatched. Use it for values you cannot know at creation time and that are only settled by running — a flow that spawns a child agent and then talks to that child, for instance. The task you reference must be listed in `--depends-on`, otherwise creation is rejected. The shape of the value is preserved. A string that is nothing but a single placeholder turns into a number if the value is a number.

## Sending a typed group of tasks at once

To fix the types of results too, bundle tasks into one graph and send it. The whole graph is checked first, so if anything is wrong no task is created and the error says which spot in which task is wrong. No task runs before every task exists. One graph holds up to 1000 tasks; split larger sets into several graphs.

```sh
tasty agent task-graph-submit --workspace 2 --graph @graph.json --dry-run   # check only
tasty agent task-graph-submit --workspace 2 --graph @graph.json
```

```json
{"contract_version": 2,
 "tasks": [
   {"id": "build", "command": {"kind": "run", "command": ["cargo", "build"]}},
   {"id": "report", "command": {"kind": "run", "command": ["notify"]},
    "input_schema": {"type": "object", "fields": {"code": {"type": "int64"}}},
    "bindings": {"code": {"from_task": "build"}},
    "input_mapping": {"args": ["/code"]}}]}
```

You choose each task's `id`. `bindings` say where inputs come from: a fixed value (`literal`), one value from an earlier task's result (`from_task` with `pointer`), or whichever of a task and its fallback ran (`one_of`). Types that do not fit are rejected when you send the graph. Values go only where `input_mapping` says. `args` appends them to the command one argument each, and `stdin: true` writes the whole input as JSON to standard input. A `wait_barrier` task can take the name of the barrier to wait on from its input with `"barrier": "/<field>"`; leave `name` out of the command then. `$(...)` or spaces inside a value are never interpreted again. The values actually passed are in the task's `input_snapshot`. Tasks in such a graph do not use the placeholders above.

To hand an existing file to a `run` task, put `{"file": "/<field>"}` in `args`. The string in that field is the file's path, and the path is appended to the command as an argument. Just before the task runs, Tasty checks that it can read the file; if the file is missing or unreadable, the command does not run and the task fails at the input stage. A relative path is accepted only when the command has a `cwd`, and is taken from there. Tasty never creates or deletes the file, so preparing and removing it is up to the tasks around it or your script. The script below writes a diff to a file, passes it to a checker, and removes it at the end even if the check fails.

```sh
#!/bin/sh
# lint-diff.sh <workspace id>
set -eu
diff_file=$(mktemp)
git diff > "$diff_file"
tasty agent task-graph-submit --workspace "$1" --graph "$(cat <<JSON
{"contract_version": 2, "tasks": [
  {"id": "lint", "command": {"kind": "run", "command": ["my-lint", "--diff"]},
   "input_schema": {"type": "object", "fields": {"diff": {"type": "string"}}},
   "bindings": {"diff": {"literal": "$diff_file"}},
   "input_mapping": {"args": [{"file": "/diff"}]},
   "on_failure": {"kind": "continue_downstream"}},
  {"id": "cleanup", "command": {"kind": "run", "command": ["rm", "-f", "$diff_file"]},
   "depends_on": ["lint"]}]}
JSON
)"
```

To run a task's result through another command (a judge or summary tool, say) and use that as the result, add `postprocess`. When the main work succeeds, the command runs and its standard output becomes the task's result.

```json
{"id": "judge",
 "command": {"kind": "run", "command": ["make-draft"]},
 "output_schema": {"type": "enum", "values": ["pass", "revise"]},
 "postprocess": {"command": ["judge-cli", "--json"],
                 "stdin": {"draft": {"from": "raw", "pointer": "/execution/stdout/text"}},
                 "stdout": {"pointer": "/verdict"},
                 "timeout_ms": 120000,
                 "retry": {"max_retries": 1}}}
```

- The command runs as is, without a shell. If you need one, write it out, as in `["sh", "-c", "..."]`. Sign-in uses the environment Tasty was started from. If you started Tasty inside Claude Code, the variables that point at that session are not passed on (the same as in the terminal). If you started Tasty from another Tasty's terminal, variables such as `TASTY_SURFACE_ID` that point at that Tasty's terminal are not passed on either.
- `stdin` sets the fields of the JSON written to standard input. `from` is `input` (the task's input) or `raw` (the main work's result), and `pointer` picks a value inside it.
- By default standard output must be exactly one JSON value. With `"stdout": {"format": "text"}` the text is taken as is. Progress logs sent to standard error stay out of the result.
- `cwd` sets the postprocess command's working folder. If omitted, it is the main work's `cwd`, or failing that, the folder Tasty was started from.
- `timeout_ms` is required. When it runs out or the task is cancelled, the command and the processes it started are stopped.
- With `retry`, a failure reruns only the postprocess that many times; the main work does not run again. `max_retries` is 1 to 10, and `delay_ms` sets how long to wait before running it again (default 0, at most 3600000). Once the retries are used up, the task fails and the tasks after it follow their failure policies as for any failure. There is no way to rerun only the postprocess. To try again, rerun the task from the main work with `task-retry`.
- While a task with declared types runs, its step appears at the end of its `task-list` line and on the `phase:` line of `task-get`. During the main work it is `executing`.
- A postprocess goes on `run` and `custom` tasks. It can't be added to `reduce` or `wait_barrier` tasks.
- A step change sends no event. If you need a signal at some point, add a `run` task after that task, such as `["tasty", "notify", "build finished"]` or a `curl` call to a webhook.
- Until the postprocess finishes, the task shows as running and the next tasks wait. `task-get` shows `phase: postprocessing (run 1)` on the line after `state: running` (`phase: retry_wait (run 2)` while it waits to retry).
- If the postprocess fails, the `state` line in `task-get` gives the reason. The lines under it give the last run's cause and exit code, such as `postprocess: run 2 failed (nonzero_exit), exit_code 3`, and the causes of the runs that were retried (`postprocess retried after: ...`). If the postprocess result is too large to store, a successful postprocess is kept as a success without the main work's response in the raw result of `task-get`. If it is still too large, or the run failed, the cause is `result_too_large` and the postprocess is not run again, even if retries are left.
- If you quit Tasty while a postprocess runs, Tasty ends the postprocess command and the processes it started. After the restart the postprocess is not run again and the task fails. Stopping the runner (`task-run --action stop`) ends a postprocess the same way, and the task shows as running until you start the runner again. To try again, rerun the task with `task-retry`.
- If Tasty is killed (with `kill`, for example), it depends on the OS. On Linux only the postprocess command itself gets a termination signal. That usually ends the command, so after the restart the task fails because its result cannot be collected, and other processes the command started can be left running. With `["sh", "-c", "sleep 300; echo x"]`, for example, `sh` ends and `sleep` is left running. On Windows they end with Tasty, and on macOS they are all left running.
- If the postprocess command is still running after Tasty restarts (Tasty was killed on macOS, or the command ignored the termination signal on Linux), the task stays running and keeps its semaphore or lease until the command ends. When it ends, its result cannot be collected, so the task fails and gives the resource back then. Tasty does not wait when only processes the command started are left. A task that was waiting to run its postprocess (including a wait before a retry) keeps its resources and runs the postprocess as scheduled.

While Tasty runs on [temporary memory](cli-reference.md#memory-shared-between-agents) because it could not open its memory file, a graph would not survive a restart, so sending it as is gets refused. For a graph that may be lost on restart, add `"durability": "best_effort"`.

### Choosing the next task from a result

Give a task `transitions` and it picks which tasks run next from its result. In the example below, `review` runs only `ship` when its verdict is `pass`, only `fix` when it is `revise`, and only `human` otherwise.

```json
{"id": "review", "command": {"kind": "run", "command": ["review-tool"]},
 "output_schema": {"type": "object", "fields": {
   "verdict": {"type": "enum", "values": ["pass", "revise", "review"]}}},
 "transitions": {
   "cases": [
     {"when": {"compare": {"path": "/verdict", "op": "eq", "value": "pass"}}, "to": ["ship"]},
     {"when": {"in": {"path": "/verdict", "values": ["revise"]}}, "to": ["fix"]}],
   "otherwise": ["human"]}}
```

- A condition compares one spot in the result (`path`) with a value (`compare`: `eq`, `ne`, `lt`, `le`, `gt`, `ge`) or checks that it is one of a list (`in`). Combine them with `all`, `any` and `not`. Conditions that do not fit the result's type are rejected when you send the graph. Decimals are compared only as ranges.
- In a `reduce` task that gathers results (strategy `all`), add `"input": "<input task id>"` to a condition to read that input task's result, in that task's result type, instead of the gathered result.
- By default exactly one condition may hold. If two hold, the task ends as failed. To run every match, add `"mode": "all_matches"`.
- Always say what happens when nothing holds: tasks to run (`otherwise`) or run nothing (`"no_match": "finish"`).
- Tasks that were not chosen, and tasks reached only through them, end as "not selected", not as failures. A task where branches meet waits only for the branches that ran. If a branch that ran fails, that failure still passes on.
- Taking a required input from a branch that may not run is rejected when you send the graph. If each branch produces the value in a different task, take it with `one_of`. If the value may be missing, make the input field optional or give it a default.
- A task's `route` shows the tasks it chose, and a task that was not chosen shows why in `skip`. When every task succeeded or was not selected, the DAG shows as succeeded. A failed task whose fallback succeeded in its place does not make the DAG fail.
- When one branch fails, the DAG still shows an in-progress state (running, ready or waiting) while another branch can go on. A task whose result became unknown (`unknown`) only moves on after you retry or cancel it, so it does not count as in progress. When everything has finished, a DAG where some branch succeeded all the way to its last task is `partially_failed`, and one with no such branch is `failed`. A single line of tasks where only the early tasks succeeded and the last one failed or was skipped is `failed`. A last task that failed but whose fallback succeeded in its place counts as a success. This appears as `rollup_state` in `agent dag-list`, and the Task DAGs list and the DAG tab show it with the `◒` mark and the label **Partially failed**.

### Asking an agent and taking its answer as the result

An `agent` task sends one instruction to an agent session such as Claude or Codex and takes the answer as the task's result. The result can feed the next task's input or the branch conditions above.

```json
{"id": "review",
 "command": {"kind": "agent", "provider": "claude", "workspace_id": 2,
             "session": {"kind": "existing", "surface_id": 12},
             "instruction": "Review the changes on this branch",
             "timeout_ms": 1800000},
 "output_schema": {"type": "enum", "values": ["approve", "revise"]}}
```

- `provider` names the plugin that runs the session: `claude` for the Claude plugin and `codex` for the Codex plugin. Other plugins that support agent tasks use their own name. If that plugin is not installed, is turned off, or lacks the turn-report permission, creating the task is refused with the reason. If you turn the plugin off after creating the task, the task fails when it runs.
- The `workspace_id` in `command` is the workspace a new session opens in. `task-submit` hands the value to the workspace the task lives in, and the guidance appended to the instruction carries that number.
- With `session` set to `{"kind": "new", "parent_surface": <surface>}`, a new session starts under that surface and gets the instruction. Add `"cwd": "<path>"` to set the new session's working folder. With `{"kind": "existing", "surface_id": <surface>}`, the instruction goes to a session that is already open.
- An open session gets the instruction only when it is idle. If you are talking to it, the task waits until you finish, so the conversations do not mix. A one-line marker for that run is added to the end of the instruction, and only a turn started with that marker counts as the task's turn. Tasks that use the same session run one after another.
- Without a result type, the last answer of the turn becomes a string result.
- If the Claude session has a Stop gate, the turn does not end while the gate keeps the response going. The task finishes with the last answer after the gate lets the response end. If you need the first answer to the instruction, set a result type and have the agent hand it in with `task-submit`.
- With a result type, as in the example, the agent has to hand in a value with `tasty agent task-submit`. How to do that, the type and a token used only for that run are added to the end of the instruction. A value with another token is refused, so a value handed in by mistake for another task or an earlier run does not get mixed in. The value is checked right away and becomes the result when the turn ends. A verdict such as `revise` is a successful result.
- A task that takes input adds `"input_mapping": {"input_block": true}`. The input is appended to the instruction as JSON.
- The task fails when the turn ends without an answer, the turn ends with an error, the session ends, or `timeout_ms` passes. `task-get` shows which one happened. When the turn ends without the value being handed in, the first 2000 characters of the turn's last answer stay in the failure record.
- While the agent waits for input such as a permission prompt, the task stays running and `task-get` shows a `phase: awaiting_input` line and a line like `agent session: claude surface 12, awaiting input since …`.
- When Tasty is busy and the reply to an instruction sent to an open session comes late, the instruction is not sent again. The task waits for the late reply so the same instruction does not arrive twice, and sends it again only when it certainly was not sent yet. If it can no longer tell whether the instruction was sent, the task becomes `unknown` rather than failed. Check whether the session got the instruction, then retry or cancel the task.
- Finishing or cancelling the task does not close the session. If Tasty restarts, running agent tasks fail. A task that was waiting for the reply to its instruction becomes `unknown`.
- Other tasks keep going while a new session is starting. If a new session does not come up within 30 seconds, the task fails. A session that comes up later is closed by the task. Cancelling the task while it waits also closes a session that comes up later. A session that was already open is never closed. If Tasty restarts while the task waits, the task becomes `unknown`.
- When Codex runs in a sandbox that blocks the network, `task-submit` cannot reach Tasty, so a task with a result type fails. Tasks without a result type (the last answer is the result) are not affected.
- To deliver the result of an existing task to a Codex parent automatically, see [Automatic delivery to a Codex parent](claude-codex.md#automatic-delivery-to-a-codex-parent).

### Example: implement, review, branch on the verdict, and meet again

This graph puts the features above together. `implement` hands out a sentence describing its change, and `review` takes that sentence as input and returns a verdict with a confidence. A `pass` with confidence 0.9 or more goes to `ship`, a `revise` with confidence 0.9 or more goes to `fix`, and anything else goes to `human_review`. The three branches meet again at `report`. Replace `make-change`, `review-tool` and the rest with your own tools. To hand the work to an agent, use the `agent` task above instead.

```json
{"contract_version": 2,
 "types": {"ReviewResult": {"type": "object", "fields": {
   "verdict": {"type": "enum", "values": ["pass", "revise", "review"]},
   "confidence": {"type": "float64", "min": 0, "max": 1}}}},
 "tasks": [
  {"id": "implement", "command": {"kind": "run", "command": ["make-change"]},
   "output_schema": {"type": "string"},
   "postprocess": {"command": ["summarize-tool"], "timeout_ms": 10000,
                   "stdin": {"text": {"from": "raw", "pointer": "/execution/stdout/text"}}}},
  {"id": "review", "command": {"kind": "run", "command": ["review-tool"]},
   "input_schema": {"type": "object", "fields": {"summary": {"type": "string"}}},
   "bindings": {"summary": {"from_task": "implement"}},
   "input_mapping": {"args": ["/summary"]},
   "output_schema": {"ref": "ReviewResult"},
   "transitions": {"cases": [
     {"when": {"all": [{"compare": {"path": "/verdict", "op": "eq", "value": "pass"}},
                       {"compare": {"path": "/confidence", "op": "ge", "value": 0.9}}]}, "to": ["ship"]},
     {"when": {"all": [{"compare": {"path": "/verdict", "op": "eq", "value": "revise"}},
                       {"compare": {"path": "/confidence", "op": "ge", "value": 0.9}}]}, "to": ["fix"]}],
     "otherwise": ["human_review"]}},
  {"id": "ship", "command": {"kind": "run", "command": ["ship-tool"]}},
  {"id": "fix", "command": {"kind": "run", "command": ["fix-tool"]}},
  {"id": "human_review", "command": {"kind": "run", "command": ["notify-reviewer"]}},
  {"id": "report", "command": {"kind": "run", "command": ["report-tool"]},
   "depends_on": ["ship", "fix", "human_review"]}]}
```

When `review` returns `pass` with confidence 0.95, `task-get` shows:

```text
state: succeeded
route: ship
attempt: review#1
revision: 7
input: summary <- implement (attempt implement#1)
output: {"confidence":0.95,"verdict":"pass"} (from postprocess.stdout.json)
```

- The `input:` line tells you which run of which task the value came from, and the `output:` line tells you where the result came from. A task that ended without a result shows `output: none`, with the failing stage and reason on an `error:` line.
- `fix` and `human_review` end with `skip: branch_not_selected`, and `report` waits only for `ship`, which ran, and succeeds.
- If `review` fails, the three branches and `report` are all skipped, and the cause shows as in `skip: upstream_unavailable (review failed)`.
- `revision` grows each time the task's record is saved. Use it to tell which of two reads of the same task is the later one.
- `confidence` here is a value the task declared in its result type, so branch conditions can use it. The `confidence` returned by a terminal state query is how sure Tasty is about the session's state, which is a different thing.

Tasks without a declared type can live in the same Workspace. A task made with `task-create --depends-on report` runs once `report` finishes. Reading a typed task's result through a placeholder such as `${task.review.output}` is rejected when you create the task, though. To pass a result along, put the receiving task in the graph too and take the value with `bindings`.

## Keeping and reading a run report

A typed group of tasks keeps a run report for people to read later. Each task has a part Tasty fills in automatically and notes the task writes itself while it runs.

- Automatic part: the final state, start and end times, the input actually received and the result produced, the failing stage and reason if it failed, and the reason if it was skipped. Shell command tasks add the exit code, agent tasks the session, and result-gathering tasks the method and number of inputs.
- Notes: a shell command task, the postprocess command that shapes its result, and a hand-written result-gathering shell get the environment variable `TASTY_TASK_REPORT`. Inside them, write `tasty agent report append 'skipped 3 tests'`. The command prints nothing when it succeeds, so it does not change the task's result; only a failure writes its reason to standard error. A shell command can also print a standard error line that starts with `::tasty-report::<note>` (the line stays in the standard error as well). An agent task gets, at the end of its instruction, the command that explains how to write notes (`tasty agent report usage`) and its address.

```sh
#!/bin/sh
# inside a shell command task
tasty agent report append "checked $(ls src | wc -l) files"
echo "::tasty-report::rebuilt the cache" >&2
```

Notes are accepted only while the task runs; after it ends they are refused. A note over the limit (1 KiB by default) is cut and marked as cut. Once one run's notes pass their total limit (16 KiB by default), further notes are only counted, not stored. Either way the command does not fail. Change both limits in **Settings** › **Misc** › **Task pipeline**. When you run a task again (`task-retry`), each run keeps its own notes. Notes are only a record, so a later task cannot take them as input (`bindings`).

```sh
tasty agent dag-report d:review-loop                       # the whole group
tasty agent dag-report d:review-loop --task build --attempt 2   # the second run of one task
tasty agent dag-report d:review-loop --include-raw         # with standard output and error
```

Find the DAG id with `tasty agent dag-list`. Earlier runs that were retried keep their own automatic part, saved when the retry started, alongside their final state and notes.

## Watching progress

The task graph is a view of progress. Create and change tasks through the CLI. Check each node’s execution state together with the result produced by the task.

There are two screens for seeing how the work flows. Both look at the same data.

- **Task DAGs** window — `Ctrl+Shift+G` (`Cmd+Shift+G` with the Mac preset), or the sidebar **Tools** menu. It is for picking one from a list, taking a quick look, and closing it. It has search and a state filter. Pick one and the same area becomes the graph, with zoom in and out, fit to view, direction switching, and the runner pill sitting on the top line next to the back arrow.
- **DAG tab** — a graph that takes up a whole Tab and stays open. Open it with `tasty new tab --pane <ID> --type dag_graph`, or press `Alt+'` on an existing Surface and switch it to **DAG**. It has zoom in and out, fit to view, and direction switching, and clicking a node shows the command, dependencies, elapsed time, exit code, and output.

The lines in the graph differ by relation. A dependency that only sets the order is solid, an input connection (`bindings`) is a teal dash-dot line, and a branch chosen from a result (`transitions`) is a lavender dashed line. A chosen branch is drawn thicker. Paths that were not chosen or will not run use a neutral color without reduced opacity, while keeping the solid, dash-dot or dashed pattern for their relation. When two tasks have both an order dependency and an input connection, only the input connection is drawn. A task skipped because its branch was not chosen is marked **Not selected**. Hovering over a node shows the task name and state, plus one more line with the reason when the task was skipped, has an unknown result, or is waiting for input. `task-graph --format dot` draws the same distinctions.

The small picture at the top left of a node is the task kind: a terminal for a shell command, a plug for an internal call, stacked layers for collecting results, a padlock for a wait, and a speech bubble for one Claude or Codex turn. Claude and Codex share the speech bubble; click the node to see which one ran it in the details.

A running task looks different depending on what it is doing. When a Claude or Codex session is waiting for a person's answer, the whole node turns yellow and shows the `!` mark, the label **Needs input**, and how long it has waited. Click that node and the details have an **Open session** button that takes you to the session's Tab so you can answer right away. Pressing it from the Task DAGs list closes the list. In the list, the row of a DAG with a task waiting for input gets `! 1 needs input`. A task that is post-processing its result or waiting to try again keeps the running colour and adds which run it is on, as in **Postprocess · run 1** and **Retry wait · run 2**.

Each list row and the top line of a DAG tab show finished tasks out of the total. When some tasks were skipped, the count follows, as in `· 2 skipped (1 not selected)`; the parenthesis appears only when some of them were skipped because another branch was chosen.

You can run several unrelated graphs in one Workspace. The list groups a chunk connected by dependencies into a single DAG. Attach `--metadata '{"dag":"name"}'` to a task and everything with the same name is grouped together regardless of whether it is connected.

| State | Meaning |
|---|---|
| **Waiting** | A task it depends on has not finished yet |
| **Ready** | It meets the conditions to run and is waiting for the runner |
| **Running** | Running |
| **Succeeded** · **Failed** | Finished |
| **Cancelled** · **Skipped** | A person cancelled it, or something before it failed and it was skipped |
| **Not selected** | Another branch was chosen, so it did not run. This is not a failure |
| **Needs input** | A running Claude or Codex session is waiting for a person's answer. The DAG state stays running |
| **Unknown** | The result is not known. Click the node to see the reason in the details. The command ended while Tasty was restarting, or Tasty kept watching it after the restart and it ended later; either way its exit code could not be collected (a restarted Tasty cannot receive that command's exit code). Later tasks wait until you run it again (`task-retry`) or cancel it. Tasks holding a semaphore or lease behave the same way: if the command is still running, it keeps the resource until it ends and then gives it back |

To see it from a terminal:

```sh
tasty agent dag-list                                   # DAGs across every Workspace
tasty agent task-list --workspace 2 --state waiting,ready,running
tasty agent task-get --workspace 2 --id "$BUILD"
tasty agent task-graph --workspace 2 --format dot   # draw it with Graphviz
```

`--format dot` writes only the graph to standard output, so you can hand it straight to Graphviz (for example `... --format dot | dot -Tsvg > graph.svg`). A cycle warning and the runner status go to standard error. `dag-get --format dot` works the same way.

`task-list` and `task-get` print lines for people to read. To use them from a script, or to see the input a task received (`input_snapshot`) and its raw result (`typed_result.raw`), add `--json`. The response from Tasty is printed as JSON as it is.

## Waiting and fixing up

```sh
tasty agent task-await --workspace 2 --id "$TEST"             # wait until it finishes
tasty agent task-retry --workspace 2 --id "$TEST"             # retry a failed, cancelled, skipped or unknown task
tasty agent task-cancel --workspace 2 --id "$TEST"
tasty agent task-set-result --workspace 2 --id "$MANUAL" --state succeeded
tasty agent task-purge --workspace 2 --states succeeded
```

If a save response is lost, Tasty recovers pending completion notifications after checking the committed record. This also wakes waits with no time limit. An unknown save result remains an error, and checking it does not rerun the command. The same completion event can be delivered again across a restart; tools consuming events should deduplicate by `terminal_id`. If the record cannot be read, Tasty preserves it and checks again on the next task change or restart.

Tasks cancelled while the runner is off continue their cleanup even after you receive their completion notifications. An earlier response from another task does not skip cleanup of the cancelled command or its downstream tasks. Holdings are returned only after termination is confirmed.

- `task-await` waits up to 10 minutes by default and comes back with a timeout if the task has not finished by then. With `--timeout-ms 0` it waits indefinitely.
- The `outcome` in the `task-await` reply is `terminal` (finished, with `state` such as succeeded or failed), `timed_out`, or `not_found`. All three exit with status 0, so do not judge success by the exit status; read `outcome` and `state`. The root option `--response-timeout-ms` only limits how long the CLI waits for a reply; it is not the task wait time.
- Read a barrier with `tasty agent barrier-state`. It returns immediately and does not wait for the barrier to close. The older name `barrier-await` gives the same result.
- For a typed task, `task-retry` is refused once its fallback has run. If the main task succeeded again, a task that takes either of the two would see two values. To run it again, send it as a new task.
- `task-retry` is refused for a task skipped because its branch was not chosen (**Not selected**). Running it again would skip it again for the same reason.
- Typed tasks do not take `--reset-downstream`. The tasks after them already finished on the earlier run's failure or chosen path. `task-retry` runs only that task again. To rerun the tasks after it, send a new graph.
- `task-set-result` is for reporting that something the runner did not run is done — a check a person does by hand, for example.
  - A typed task gets an attempt ID (`<task ID>#<number>`) each time it runs. Name the attempt with `--attempt-id`. If the task has run again since, the old report is refused instead of finishing the new run.
  - Sending the same report again returns the same answer as the first time. A different result for an attempt that has already finished is refused.
- Cancelling a running command task with `task-cancel` stops the command and the processes it started. This includes the case where the command has already ended but a process it started still holds its output, so the task still shows as running. A daemon that closed its output and stays behind is not treated as part of the task once the task ends. The semaphore or lease is given back only after they have ended, so a task waiting for the same resource starts after the earlier command has really stopped. This also starts right away when the runner is off. In that case the cancel command returns without waiting for them to end, and the semaphore or lease is given back once they have. If you run it again with `task-retry` before that, the task waits in the ready state until the runner confirms that every process from the earlier attempt has ended. While it waits, `task-list` adds `(waiting_previous_attempt)` at the end of its line and `task-get` shows a `phase: waiting_previous_attempt` line. Delayed cleanup from an earlier cancellation leaves a newer running attempt and its resource holdings alone. On Windows, for a command started before Tasty restarted, only that command itself is stopped, so processes it started may stay behind.
- `task-delete` is refused while another task references it, and tells you the ID of the referencing side. A running task has to be cancelled first. It is also refused while the processes of a cancelled task have not yet been confirmed ended. You can delete it once they have ended and the semaphore or lease has been given back. `task-purge` skips such tasks and reports them in `skipped` and `skipped_count` in its response.

## Receiving finished work as events

Instead of asking again and again whether a task is done, you can have it come to you.

```sh
tasty events follow --filter 'agent.*'
```

One event per line as JSON, so a shell loop can read it directly.

```sh
tasty events follow --filter 'agent.*' | while read -r line; do
  echo "event: $line"
done
```

- **Save your read position on the receiving side.** Pass the `next_offset` that comes back as the next `--offset` and you carry on from where you stopped. For a single read rather than a loop, use `tasty events fetch --offset <number>`.
- Events live in memory only, and only the most recent ones are kept. If you were away long enough for the ones in between to fall out, you are **not** quietly given the oldest ones instead — you are told how many were missed. That notice is kept out of the event stream, so the `while read` above is not disturbed.
- Restarting Tasty clears the events and starts positions over. If the `epoch` that comes with an answer differs from the one your position came from, that position belongs to the previous Tasty run.
- If the connection drops, `follow` tells you the `--offset` and `--epoch` to reattach with and exits. Run it again with those, and if Tasty restarted in the meantime you are told so and get the new events from their start. With `--reconnect` it does not exit but reconnects every second and carries on.
- What comes out today is **a task finishing** and **a barrier closing**. A task event carries its state together with the run (`attempt_id`), the record version (`revision`) and why it was skipped (`skip`, with the task and state that caused it when an earlier task was the reason). Why something failed is not carried in the event — use `tasty agent task-get` for that.
- Tasty does not keep growing a separate queue for a slow reader. Check for missed events if the reader falls behind the shared recent-event history.

## Concurrency limits and signals

Coordination devices for running several tasks at once come along with it.

| Device | Use |
|---|---|
| Semaphore | Decides how many tasks carrying the same name may run at once. `--concurrency-limit <name>` at task creation is the short form. As with a lease, the runner keeps pushing back the expiry of a semaphore attached to a task while it watches the task |
| Barrier | Blocks until the set number of signals have gathered. Slot it into the graph as a `wait_barrier` task |
| Lease | Makes something like a file be held by only one holder at a time. It has an expiry, and on a conflict it either fails or returns at once saying it was not acquired — it does not wait. For a lease attached to a task (`metadata.lease`), the runner keeps pushing the expiry back while it watches the task, so a long task does not lose its lease midway. It is not pushed back while the runner is off. The expiry attached to a task must be at least one second. If it still runs out before being pushed back and someone else takes the lease, `task-get` shows a `warning:` line. It covers only the current attempt, so after you run the task again the earlier attempt's warnings no longer show |
| Reducer | Merges the results of several tasks into one. Choose between first success only, all of them, JSON merge, or text concatenation, or merge them yourself with `custom:<shell command>`. That command reads the input results as a JSON array on standard input; if its standard output is JSON that value is the result, otherwise the text as it is |
| Rate limit | Decides how many times per period is allowed, per agent and per metric |

```sh
tasty agent semaphore-create --workspace 2 --name build --permits 2
tasty agent barrier-create --workspace 2 --name ready --count-required 3
tasty agent lease-acquire --workspace 2 --resource file:/tmp/db --holder agent-a --ttl-ms 60000
tasty agent task-reduce --workspace 2 --inputs "$A,$B" --strategy all --extract-path /stdout/text
```

Semaphore and barrier names use only lowercase letters, digits, `.`, `_`, and `-`. If a name contains any other character, the command is rejected and tells you which character is the problem. Lease resource names have no such limit.

The full list is in `tasty agent --help`.

<a id="what-to-read-next"></a>

## Keep exploring

- [Driving Tasty from the CLI](cli.md) — The commands agents use in general
- [Claude · Codex](claude-codex.md) — Spawning child agents and receiving completion notifications
- [Hooks, notifications, and webhooks](hooks-notifications.md) — Getting notified when a command finishes
