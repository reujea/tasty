<!-- source-hash: 587f7c7f1a17 -->
# CLI response delays and connection diagnostics

Read this page when a `tasty` command replies slowly, cannot connect, or ends with an error such as `Error (-32061)`. It covers the difference between the reply wait bound and the task wait time, what to do for each error code, checking what this Tasty supports, and the connection counts, request queues and database state shown by `tasty list pressure`. If you are new to the CLI, start with [Driving terminals with the tasty CLI](../agents/cli.md).

## Reply wait versus task wait

- `tasty --response-timeout-ms MS <command>` bounds how long the CLI waits for Tasty's **reply**. Put it before the command.
- `tasty agent task-await --timeout-ms MS` is how long to wait for the **task** to finish. Without it the wait is 10 minutes; `0` waits indefinitely. The result is reported in the reply's `outcome`, and a timeout also exits with status 0 ([Waiting and fixing up](../agents/tasks.md#waiting-and-fixing-up)).
- `tasty pty state` and `tasty agent barrier-state` do not wait; they return the current state right away. To wait, call them again.

When you give both, a reply bound shorter than the task wait makes the CLI end with `Error (-32061)` before the task finishes. Set the reply bound longer than the task wait.

## Bounding how long to wait for a reply

```sh
tasty --response-timeout-ms 5000 read screen --surface 42
```

Put `--response-timeout-ms` **before** the command. If no reply comes within that time, the command ends with `Error (-32061): …`. That error means **the outcome is unknown** — the request may keep running inside Tasty, so if the command changes something, check the state before sending it again. If the time runs out while the request is still waiting its turn inside Tasty, the command ends with `Error (-32067): …` instead. Then **nothing ran**, so you can send it again as is. Without the flag, or with `0`, there is no bound.

It only works for commands that send a single request; commands that ask repeatedly, like `events follow`, or open a connection, like remote attach, refuse the flag with exit code 2 and send nothing.

An older Tasty that does not understand the bound is not sent the request. The command then writes one line, `{"error":{"kind":"unsupported_capability",…,"sent":false}}`, to stderr and exits with code 1 ([Checking what this Tasty supports](#checking-what-this-tasty-supports)). That check counts against the same time: if Tasty is stuck and even the check does not finish in time, the request is not sent and the command ends with `Error (-32067): …`, so you can send it again as is.

## Checking what this Tasty supports

`list info` also answers what this Tasty can do, under `capabilities`. Each entry pairs a name with a version, and it tells you what the version string alone cannot — the same version can do different things depending on how it was built. Ignore any name you do not recognise.

When a command ends with an error line containing `"sent":false`, the Tasty you are connected to is an older version that does not know that feature, so the request was not sent. The name under `capability` says what is missing.

## What to do for each error

- **An error line containing `"sent":false`** — the Tasty you are connected to is an older version that does not know that feature (reading from a position, bounding the reply wait, and so on). The request was not sent. The name under `capability` says what is missing.
- **A `data: {…}` line follows the `Error (…)` line** — Tasty sent a classification of the failure along with it (for example `storage_failure` when a memory write fails, or `reason` for a refusal). Everything after `data: ` is one line of JSON, so a script can branch on that value instead of the first line. The first line, `Error (code): message`, is the same whether or not this line is present. `tasty events follow` and `tasty plugin audit-follow` add this line too; their first line keeps its shape, `Error: Error (code): message`, with one extra `Error: ` in front.
- **The command ends with `Error (-32603): memory writer queue is full …`** — Too many memory storage requests arrived at once, so Tasty did not take this one (`storage_failure` in `data` is `busy`). Nothing was stored, so call it again after a short pause. `memory writer is not accepting requests` means Tasty is shutting down.
- **A request from an agent using a session token ends with `Error (-32000): host is busy: the request gate could not reach the memory writer …`** — Tasty could not queue the session check it runs before taking the request (`storage_failure` in `data` is `busy`). The request did not run, so call it again as is after a short pause. `Error (-32000): host is shutting down` means Tasty started shutting down and did not run the request. CLI commands without a token skip this check.
- **A memory or agent coordination command ends with an error containing `"outcome":"unknown"`** — Tasty could not confirm whether the write finished. Check the value with `tasty memory get` or similar before sending the same request again. Stopping the command while it waits does not cancel a storage request Tasty already took.
- **The command ends with `Error (-32065): …`** — Tasty has a backlog of requests to handle and did not take this one. The request did not run, so call it again as is after a short pause.
- **The command ends with `Error (-32066): …`** — nothing was sent within 20 seconds of connecting, so Tasty closed the connection. Nothing ran. If your tool opens the socket itself, send the request right after connecting. A connection that has sent one request is not closed however long it pauses between requests.

## How requests are handled

For agents using session tokens, call limits apply to plugin commands and combined list queries as well as ordinary commands. Each admitted request is counted once; requests over the limit return an error without running. The existing exemption for local CLI calls without a token remains.

A plugin namespace call starts only its enabled owner and any active extension needed for matching IPC hooks. It does not enable a disabled plugin or start unrelated plugins. An unknown namespace starts none; a misspelled method inside a known namespace may start its owner before returning an error.

Structural requests rejected by a queue or storage budget limit return `-32065`. Requests can wait within the queue limit when other requests temporarily hold the required storage credit. Reusing an idempotency key for a different request returns `-32063`.

While a structural change in one window waits for an external operation, an independent structural request targeting another window can proceed. Requests affecting the same target scope keep their order.

## Response delays and connection status

`tasty list pressure` shows request wait and processing times, connection counts, and refusal reasons.
Each section measures different operations; do not subtract their request counts to calculate refusals.
Totals accumulate from startup. An average is `null` when nothing has been measured.
Current usage and configured limits are identified separately below.

| Timing section | What it measures |
|---|---|
| `queue_before_gate` | Queue wait, including requests later rejected by permission checks |
| `handler_after_gate` | Processing time for requests that passed those checks |
| `plugin_round_trip` | Wait time for plugin requests that received a response |
| `db` | Time spent on database commits and WAL checkpoints, the number of writes saved by each commit, and VACUUM runs counted separately |

For long queue waits, check for a backlog. For long processing times, identify the commands involved.
For long plugin waits, inspect the plugin’s log. Database timings help investigate storage work and
contention; a high value alone does not prove a disk problem. Commit time is already included in
handler time, so adding them double counts it. `checkpoints_busy` counts checkpoints that could not
finish because another connection was using the database. `writes_per_commit` is the distribution of
writes saved by each commit, and database compaction (VACUUM), which is not counted as a commit,
is counted separately in `vacuums`. `memory_writer` counts admissions, refusals, and batches of the
queue that groups save requests. `state_writer` and `state_db` show the same counts and the commit
times for the queue that saves recent files and tutorial progress. Both are `null` in headless runs
and when `state.db` could not be opened. Permission, usage-cap, and rate-limit
refusals are counted separately in `gate_refusals` below.

The save queues expose `batch_wait` for time before a batch starts, `batch_execution` for the batch including retry waits, and `executor_execution` for individual processing calls. These distributions use microseconds; `samples: 0` means no observation yet. `queued_items` and `queued_bytes` count only work not yet dispatched. `pending_bytes` is the input size of unfinished requests, and `oldest_pending_age_us` is the age of the oldest one, or `null` when none remain. Finished requests whose results have not been read can still occupy `held_bytes`. Pending age is not the age of unsaved file changes, and completed requests or batches are not counts of physical disk commits.

`journal` shows admitted, refused, queued, and inflight journal work in both GUI and headless runs.
Work steps are distinct from user commands, and bytes are input-size estimates. Inside `store`,
`physical_commits` counts actual SQL commits, `structural_commits` counts commits containing structural
changes, and `logical_requests` counts committed logical requests. Timing distributions use fixed buckets.
A `null` `dirty_age_us` means this metric does not provide that age. `unpublished_age_us` measures
waiting for publication acknowledgement after durable storage; reading it does not reset it.
After an actual SQL commit failure, `publication_known` is `false` and `unpublished_age_us` is `null`.
The cache stays unknown until a new worker reopens and revalidates the journal.
Test configurations without a journal return `journal: null`; before the Store opens, `store` is `null`.

`file_snapshots.settings` shows the difference between settings applied in memory and settings confirmed saved, in both GUI and headless mode. When `dirty` is true, `dirty_age_us` is the number of microseconds since the first unsaved change. New changes, failures, unknown outcomes, retries, and successful saves of older changes do not reset that age. Confirmation that the latest change was saved makes `dirty` false and the age `null`. Unsaved changes can remain after a failure even when `pending` is 0. Reading these values does not collect save results.

`state_snapshots.recent_files` shows whether the GUI's recent-file lists have changes not yet confirmed saved (`dirty`) and the microseconds since that continuous period began (`dirty_age_us`). Changes to another file type, save failures, refusals, and unknown outcomes keep the age running. The age becomes `null` only after the latest list of every type is confirmed saved. This is one age for the shared cache, rather than a separate age for each type, and reading it does not collect save results. The list state is `null` in headless mode and when the state database could not be opened.


### Connection counts and limits (`connections`)

| Field | Meaning |
|---|---|
| `live` | Current connections; decreases as connections close |
| `live_max` | Highest concurrent connection count since startup |
| `limit` | Maximum allowed concurrent connections |
| `accepted` | Total connections accepted |
| `refused_saturated` | Total connections refused at the limit |

Connections count toward live even before sending a request. If live approaches limit or
refused_saturated increases, close persistent connections you no longer use, such as unused attaches.
These refusals happen before request processing and do not appear in request timings.
Connection limits and processing delays can be problems at the same time.

`accept_wait_bound_us_max` and `accept_wait_bound_us_mean` report the maximum and average upper
bounds on connection acceptance waits. `accept_waits` is the observation count, and
`accept_wait_bound_us_sum` is the sum of those bounds. These are not exact wait measurements.
They start from when Tasty last found the connection queue empty, so the actual wait can be shorter.
Tasty normally waits 100ms after finding an empty queue, but 100ms is not a strict cap on the total wait.
This time is separate from request queue time. The average can decrease as observations accumulate.

### Timing distributions

Only the first three timing sections include `wait_us_hist` or `us_hist`.
`bounds_us` contains 11 bucket boundaries from 10µs to 1 second; `counts` contains each bucket’s count.
The last bucket holds times above 1 second, so counts has one extra entry. Buckets do not overlap:
their sum is the observation count. The section’s max field shows the largest time. Percentiles
such as p99 are not provided.

`queue_before_gate.waits` counts measured waits and is the average’s denominator.
`commands` increases after a batch finishes, so it can trail waits by the batch currently running.
Use the average and distribution together to distinguish widespread delays from a few slow requests.

### Database settings (`db_pragmas`)

For memory_db and state_db, requested is the setting Tasty asked for and effective is what took effect.
These values are checked when the database opens; they are not running totals.

| Value | Meaning |
|---|---|
| `degraded: true` | Settings did not reach the expected state, or temporary memory replaced the file database |
| `state_db: null` | The state database is not open in this instance; headless instances also use this value |
| `memory_db.init_failure` | Cause and error text when a file database could not open and temporary memory was used; otherwise null |

The database can remain usable with a settings mismatch. Compare requested and effective to investigate
performance or recovery after a crash. Temporary memory makes degraded true even if all settings took
effect, and values stored there are lost after restart.

### Screen delivery (`stream_push`)

| Field | Meaning |
|---|---|
| `frames_dropped` | Total frames discarded because receivers could not keep up |
| `clients_lagged_out` | Total receivers disconnected for falling too far behind |
| `backlog` | Frames currently waiting to be sent; decreases as they are sent |
| `sink_capacity` | Maximum queued frames for one connection |

Increasing frames_dropped can mean an attach missed part of the screen.
Tasty attach clients reconnect and fetch a fresh screen when instructed to reconnect.

### Request queues (`queue_admission`, `queue_dispatch`)

queue_admission shows what is queued and what was refused before entering the queue.

| Field | Meaning |
|---|---|
| `queued_bytes`, `queued_commands`, `queued_injected` | Current queued bytes, commands, and internal commands |
| `peak_bytes` | Highest queued byte count since startup |
| `limit_bytes`, `limit_injected_depth` | Limits on bytes and internal commands |
| `refused_bytes`, `refused_depth` | Total requests refused at each limit |

Check these fields for a queue-full error. Requests refused before admission do not contribute to
queue wait or execution times.

queue_dispatch describes taking commands out of the queue and running them.

| Field | Meaning |
|---|---|
| `rounds` | Times commands were taken out for processing |
| `rounds_stopped_by_count`, `rounds_stopped_by_time` | Rounds stopped after using their command or time allowance |
| `expired_before_run` | Commands not started because the caller’s deadline passed while queued |
| `started` | Commands whose execution started |
| `in_flight`, `in_flight_max` | Current and peak requests that have started and whose command or response wait has not ended |

in_flight can decrease. A command that is still running remains counted after its caller stops waiting. Increasing time-limited rounds or expired_before_run can indicate that queue
processing is not keeping up with incoming requests.

Category creation, renaming, deletion and reordering, plus workspace information and order changes, return the original result when retried with the same key, even after the app restarts. Later moves or renames do not cause the retry to select a different target. Retrieving a saved result does not reapply a remote screen's local display name or position. Other commands do not all provide this protection across restarts.

### Duplicate requests (`keyed_requests`)

Only requests with an idempotency_key are counted, once per request.

| Field | Result |
|---|---|
| `executed` | A request with a new key ran |
| `replayed` | The same request received its earlier result without running again |
| `conflicted` | Different content used the same key, so nothing ran |
| `discarded` | The request ran but its response was discarded |
| `in_flight` | The same request was already running, so this request waited for that result |

If replayed grows faster than executed, check for repeated retries.
This in_flight count differs from the current running count in queue_dispatch.

### Individual slow requests (`slow_requests`)

Requests whose queue, host, and plugin times total at least threshold_us (100ms) appear oldest first,
up to capacity (32) records. New records replace the oldest when full. admitted counts all records
added since startup; subtract the current record count to find how many were removed.

| Field | Contents |
|---|---|
| `request_seq` | A Tasty-assigned number, separate from the request’s id |
| `host` | method, caller, queue_wait_us, host_us, outcome(ok/error), and error_code |
| `plugin_hops` | Each plugin’s host_request_id, wait time, and ok/error/expired/cancelled result |
| `total_us` | Sum of the measured times |

Before a response is sent, host outcome and error_code are null. For a long plugin_hops wait, find
host_request_id in the plugin’s log. Plugin error and missing-response logs include id and request_seq.
Request bodies and tokens are not stored, and the pressure query itself is excluded.
Restarting clears these records and starts numbering from 1 again.

### Refusal reasons (`gate_refusals`)

judged counts requests checked for permission and other limits, including CLI and plugin calls.
Each request appears in at most one refusal category.

| Field | Reason and error code |
|---|---|
| `permission_denied` | Rejected by the permission check, -32001 |
| `cap_blocked` | A telemetry usage cap was reached, -32007 |
| `throttled` | A call rate limit was reached, -32010 |

Check permissions and usage caps, or wait for the rate limit to clear.
Permission refusals also include plugin or agent calls to unavailable or unknown commands.
Asking for a permission does not fix those cases; check the command instead.
Local CLI calls without a session token retain their permission exemption.

Not every -32001 appears in permission_denied. Malformed, unknown, expired, or revoked session tokens
are rejected before these checks and are excluded from judged too. A -32001 returned after the checks,
such as a refused permission grant, is also excluded from this refusal counter.
These totals reset on restart. The throttled_count from `tasty agent rate-limit-status` is a persistent
per-limit total, so it is a different value.

## Keep exploring

- [Troubleshooting](troubleshooting.md) — Install, connection and hook problems.
- [Driving terminals with the tasty CLI](../agents/cli.md) — The first-time order and the basic pattern.
- [CLI command reference](../agents/cli-reference.md) — Finding commands beyond the introduction by purpose.
