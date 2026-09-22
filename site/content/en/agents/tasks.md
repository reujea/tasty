<!-- source-hash: 96124475f363 -->
<a id="task-dag"></a>

# Task workflows (DAG)

Run tests after a build finishes, then pass the results to the next task. Register each step as a **task** and connect it to the work it depends on. Tasty handles the order and failure policy. These connected tasks form a **DAG**, a graph of task dependencies.

Create and manage tasks with the `tasty` CLI. To follow them in the app, see [Watching progress](#watching-progress).

Pass the task's workspace as `--workspace-id` in each command. The command uses that workspace even while you are viewing another one.

## Starting the runner

You can create tasks, but nothing runs while the runner is off. Start it once per Workspace.

```sh
tasty agent task-run --workspace-id 2 --action start
tasty agent task-run --workspace-id 2 --action status
tasty agent task-run --workspace-id 2 --action stop
```

Restarting Tasty does not start the runner again automatically. The tasks themselves are still there, so `start` again and it picks up where it left off. When the runner is stopped, the runner pill on the DAG screen says so, and hovering it shows the command that starts it again. In a DAG tab that command is also spelled out on the header line.

## Creating a task

```sh
tasty agent task-create --workspace-id 2 --name build \
  --command '{"kind":"run","command":["cargo","build"]}'
```

Creating one returns a task ID. Use that ID to wire dependencies and to query state.

| Command kind | What it does |
|---|---|
| `run` | Runs the specified command. It is a background process that does not occupy a terminal, and it carries up to the last 64KiB each of standard output and standard error in the result. Interactive programs do not fit here |
| `custom` | Turns one of Tasty's own actions into a task. Things that create a terminal, such as spawning a child agent, belong here |
| `reduce` | Merges the results of several tasks into one |
| `wait_barrier` | Waits until all the signals have gathered at a barrier |

You can also pull the command JSON out into a file and pass it as `--command @build.json`.

## Order and failure handling

```sh
tasty agent task-create --workspace-id 2 --name test \
  --command '{"kind":"run","command":["cargo","test"]}' \
  --depends-on t-build --on-failure abort
```

Every task listed in `--depends-on` has to finish before this task becomes ready. A graph that would form a cycle is rejected at creation time.

| Failure policy | Meaning | Where you attach it |
|---|---|---|
| `abort` (default) | If a task it depends on fails, this task is skipped. Everything below it is skipped too | The depending side |
| `continue_downstream` | This task runs even if a task it depends on failed | The depending side |
| `fallback:<task ID>` | If this task fails, that task is woken up instead | The side that can fail |

It is easy to get the placement wrong. `abort` and `continue_downstream` have to be attached to the **following** task for them to count toward that task's readiness decision, while `fallback` has to go the other way, on the task that **can fail** itself. Using a policy on the wrong task will not give you the intended failure handling.

A task used as a fallback has to be created before the main task. To stop the runner from running it first in the meantime, create the fallback with `--reserved-for-fallback`. It then does not run until a main task that references it exists.

## Passing results from earlier tasks

`--depends-on` only ties order together. To pass an earlier task's result as a later task's **input**, use a placeholder.

```
${task.<task ID>.output}          the whole result
${task.<task ID>.output/stdout/text}   one value inside the result
```

The real value goes in that spot when the task is dispatched. Use it for values you cannot know at creation time and that are only settled by running — a flow that spawns a child agent and then talks to that child, for instance. The task you reference must be listed in `--depends-on`, otherwise creation is rejected. The shape of the value is preserved. A string that is nothing but a single placeholder turns into a number if the value is a number.

## Watching progress

There are two screens for seeing how the work flows. Both look at the same data.

- **Task DAGs** window — `Ctrl+Shift+G`, or the sidebar **Tools** menu. It is for picking one from a list, taking a quick look, and closing it. It has search and a state filter. Pick one and the same area becomes the graph, with zoom in and out, fit to view, direction switching, and the runner pill sitting on the top line next to the back arrow.
- **DAG tab** — a graph that takes up a whole Tab and stays open. Open it with `tasty new tab --pane <ID> --type dag_graph`, or press `Alt+'` on an existing Surface and switch it to **DAG**. It has zoom in and out, fit to view, and direction switching, and clicking a node shows the command, dependencies, elapsed time, exit code, and output.

You can run several unrelated graphs in one Workspace. The list groups a chunk connected by dependencies into a single DAG. Attach `--metadata '{"dag":"name"}'` to a task and everything with the same name is grouped together regardless of whether it is connected.

| State | Meaning |
|---|---|
| **Waiting** | A task it depends on has not finished yet |
| **Ready** | It meets the conditions to run and is waiting for the runner |
| **Running** | Running |
| **Succeeded** · **Failed** | Finished |
| **Cancelled** · **Skipped** | A person cancelled it, or something before it failed and it was skipped |
| **Unknown** | Cannot be determined |

To see it from a terminal:

```sh
tasty agent dag-list                                   # DAGs across every Workspace
tasty agent task-list --workspace-id 2 --state waiting,ready,running
tasty agent task-get --workspace-id 2 --id t-build
tasty agent task-graph --workspace-id 2 --format dot   # draw it with Graphviz
```

## Waiting and fixing up

```sh
tasty agent task-await --workspace-id 2 --id t-test              # wait until it finishes
tasty agent task-retry --workspace-id 2 --id t-test              # retry a failed, cancelled or skipped task
tasty agent task-cancel --workspace-id 2 --id t-test
tasty agent task-set-result --workspace-id 2 --id t-manual --state succeeded
tasty agent task-purge --workspace-id 2 --states succeeded
```

- `task-await` waits up to 10 minutes by default and comes back with a timeout if the task has not finished by then. With `--timeout-ms 0` it waits indefinitely.
- `task-set-result` is for reporting that something the runner did not run is done — a check a person does by hand, for example.
- `task-delete` is refused while another task references it, and tells you the ID of the referencing side. A running task has to be cancelled first.

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

- **The reader holds the position, not Tasty.** Pass the `next_offset` that comes back as the next `--offset` and you carry on from where you stopped. For a single read rather than a loop, use `tasty events fetch --offset <number>`.
- Events live in memory only, and only the most recent ones are kept. If you were away long enough for the ones in between to fall out, you are **not** quietly given the oldest ones instead — you are told how many were missed. That notice is kept out of the event stream, so the `while read` above is not disturbed.
- Restarting Tasty clears the events and starts positions over. If the `epoch` that comes with an answer differs from the one your position came from, that position belongs to a previous generation.
- If the connection drops, `follow` tells you the `--offset` and `--epoch` to reattach with and exits. Run it again with those, and if Tasty restarted in the meantime you are told so and get the new events from their start. With `--reconnect` it does not exit but reconnects every second and carries on.
- What comes out today is **a task finishing** and **a barrier closing**. Why something failed is not carried in the event — use `tasty agent task-get` for that.
- A slow reader queues nothing on the Tasty side.

## Concurrency limits and signals

Coordination devices for running several tasks at once come along with it.

| Device | Use |
|---|---|
| Semaphore | Decides how many tasks carrying the same name may run at once. `--concurrency-limit <name>` at task creation is the short form |
| Barrier | Blocks until the set number of signals have gathered. Slot it into the graph as a `wait_barrier` task |
| Lease | Makes something like a file be held by only one holder at a time. It has an expiry, and on a conflict it either fails or waits |
| Reducer | Merges the results of several tasks into one. Choose between first success only, all of them, JSON merge, or text concatenation |
| Rate limit | Decides how many times per period is allowed, per agent and per metric |

```sh
tasty agent semaphore-create --workspace-id 2 --name build --permits 2
tasty agent barrier-create --workspace-id 2 --name ready --count-required 3
tasty agent lease-acquire --workspace-id 2 --resource file:/tmp/db --holder agent-a --ttl-ms 60000
tasty agent task-reduce --workspace-id 2 --inputs t-a,t-b --strategy all --extract-path /stdout/text
```

Semaphore and barrier names use only lowercase letters, digits, `.`, `_`, and `-`. If a name contains any other character, the command is rejected and tells you which character is the problem. Lease resource names have no such limit.

The full list is in `tasty agent --help`.

<a id="what-to-read-next"></a>

## Keep exploring

- [Driving Tasty from the CLI](cli.md) — The commands agents use in general
- [Claude · Codex](claude-codex.md) — Spawning child agents and receiving completion notifications
- [Hooks · notifications · webhooks](hooks-notifications.md) — Getting notified when a command finishes
