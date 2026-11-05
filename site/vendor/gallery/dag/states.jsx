// Which contracts of tokens/ground-ink-rules.json this page owns, by specimen. See gallery/coverage.md.
export const STATES = {
  "task-node-every-execution-state": [
    "DAG node",
    "DAG node.running",
    "DAG node.failed",
    "DAG node.partially-failed"
  ],
  "running-phase-only-awaiting-input-gets-its-own-look": [
    "DAG node.unknown",
    "DAG node.awaiting"
  ],
  "host-runner-state": [
    "DAG runner badge.stalled",
    "DAG runner badge.crashed"
  ],
  "empty-states-and-the-cycle-warning": [
    "DAG cycle notice"
  ]
};
