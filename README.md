# Tasty

<img src="assets/icons/tasty-melon.svg" alt="Tasty logo" width="96" height="96" />

한국어: [README.ko.md](README.ko.md)

> **Tasty** is a cross-platform, GPU-accelerated terminal emulator purpose-built for AI coding agents. It provides multi-agent orchestration, headless operation, and a focus-independent IPC/CLI surface across Windows, macOS, and Linux. (Detailed docs are in Korean — start at [`docs/index.md`](docs/index.md).)

[![Version](https://img.shields.io/badge/version-0.10.4-blue)](CHANGELOG.md)
[![License: MIT](https://img.shields.io/badge/license-MIT-green)](#license)
[![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey)](docs/installation.md)
[![Workspace](https://img.shields.io/badge/workspace-60%20crates-orange)](crates/)

Where GPU-accelerated terminals like WezTerm and Alacritty focus on the human typing experience, Tasty adds another coordinate on top: a terminal an AI agent can operate directly — every surface is equally open to keyboard/mouse *and* IPC/CLI.

## Identity — Separation of User Actions and Agent Actions

Every Tasty API strictly separates **user actions** (keyboard/mouse/native OS input) from **agent actions** (IPC methods/CLI subcommands). Side effects of agent actions never touch the user's focus, history, or selection state — features that *replay* user input (key injection, forced focus switches, etc.) do not exist on the release IPC/CLI surface (debug-only isolation). Full principles: [`CLAUDE.md`](CLAUDE.md).

## Core Values

- **Cross-platform** — Windows / macOS / Linux, all native (winit + wgpu).
- **GPU-accelerated rendering** — cell-based shaders, stable prepare/draw even with 10+ surfaces.
- **Hexagonal architecture** — model + ports + adapters + view + host_api separation, 60-crate workspace.
- **AI agents as first-class citizens** — every IPC/CLI surface is focus-independent and ID-based. User actions and agent actions are fully separated (debug isolation).

## Main Systems

Three systems are built on top of the terminal. All three work from both the GUI and the CLI.

### Agent orchestration with a task DAG

Agents hand work to other agents, and Tasty runs it in order.

- Tasks form a dependency graph driven by a state machine. A task starts once everything it depends on has finished, and a dependency cycle is rejected when the task is created.
- When a task fails, its downstream tasks can be skipped, allowed to continue, or replaced by a fallback task.
- A finished task's output can be passed into a later task as input. A reduce task merges several results into one: first success, all, JSON merge, text concatenation, or a custom command.
- Semaphores limit how many agents run at once, leases mark a resource as taken, barriers wait until several agents have arrived, and rate limits throttle calls.
- A spawned Claude or Codex child can be a node in the graph. The node completes when the child goes idle or asks for input, and counts as failed if the child exits, so the failure policy above applies to it.
- Progress shows as a live graph in a tab (`tasty new tab --type dag_graph`) and as JSON or Graphviz dot from the CLI.

Details: [`docs/features/agent-collaboration/index.md`](docs/features/agent-collaboration/index.md)

### Plugins in their own processes

- Every plugin runs as a separate OS process and talks to the host over local TCP with JSON messages. The host checks that each plugin still responds, and one that stops responding is listed under "needs attention" in the plugin window.
- Plugin processes are tied to the host's lifetime at the OS level (a Job Object on Windows, a parent-death signal on Linux, a watchdog in the SDK on macOS). No plugin process is left running after Tasty exits, even after a crash.
- A plugin can add CLI subcommands, IPC namespaces, its own surface types (rendered by the plugin itself or shown in a web view), popups and tool menu entries, file handlers, settings pages, hook events, and completion rules for DAG tasks.
- Permissions such as file read and write, process spawn, network, clipboard, and terminal read and write are declared in the manifest and granted at install time. Manifests are signed with ed25519, and a plugin with an unknown key or changed permissions has to be trusted again.
- The Markdown, Image, HTML, Git, and Clipboard viewers and the Claude Code and Codex integrations that ship with Tasty are plugins built on the same SDK as third-party ones.

Details: [`docs/features/plugin-system/index.md`](docs/features/plugin-system/index.md), [`docs/dev-guide/plugin-development.md`](docs/dev-guide/plugin-development.md)

### Remote attach

- Connect to a Tasty instance already running on another machine and keep working in its workspaces from your own window. A single surface or a whole workspace can be attached. It appears locally as a mirror, and your input goes to the remote PTY.
- One client holds an attached surface at a time. A second attach is refused and told who the holder is, and input from the remote machine's own keyboard and agents is blocked while the hold lasts.
- The hold is released when the connection closes or when heartbeats stop (sent every 5 seconds, declared dead after 20). The user at the remote machine can force a detach at any time.
- Tasty adds no network protocol, authentication, or encryption of its own. The server listens on loopback only, and the client reaches it through a tunnel opened with the system `ssh`.
- A local workspace can be mapped to a remote profile and workspace, and then it attaches automatically when it is activated.

Details: [`docs/features/remote-attach/index.md`](docs/features/remote-attach/index.md), [`docs/features/remote-profiles/index.md`](docs/features/remote-profiles/index.md)

## Installation

Full instructions: [`docs/installation.md`](docs/installation.md).

Grab prebuilt binaries for macOS (DMG), Windows (MSI), and Linux (AppImage, etc.) from **[GitHub Releases](https://github.com/zilhak/tasty/releases/latest)**. The source tree can be ahead of the latest release, so build from source below if you need the newest features.

```bash
# Build from source (all platforms)
git clone https://github.com/zilhak/tasty.git
cd tasty
cargo build --release
./target/release/tasty
```

## Key Features

- **Orchestrate multiple AI agents in one terminal** — a task DAG plus barrier / semaphore / lease / reduce / rate-limit collaboration primitives coordinate parallel work ([`docs/features/agent-collaboration/index.md`](docs/features/agent-collaboration/index.md))
- **Runs fully headless** — create, tear down, and drive surface I/O with CLI/IPC alone, so it drops straight into CI/server environments (headless build: `cargo build --no-default-features`; the flag `--headless` does not make a GUI build headless, [`docs/features/headless-pty/index.md`](docs/features/headless-pty/index.md))
- **Select and copy with the keyboard alone** — vi-style copy mode (hjkl movement, visual selection, search) with GPU cursor visualization ([`docs/features/clipboard/index.md`](docs/features/clipboard/index.md))
- **Produce installers in one step** — `cargo build --profile dist` plus a Justfile wrapper auto-builds DMG / MSI / AppImage
- **Extend it yourself with plugins** — an SDK with a manifest schema and a permission system ([`docs/features/plugin-system/index.md`](docs/features/plugin-system/index.md))
- **Share context between agents** — Blackboard / Plan / Cache let multiple agents exchange the same working context ([`docs/features/agent-collaboration/index.md`](docs/features/agent-collaboration/index.md))
- **Pinpoint output per shell command** — recognizes shell prompt boundaries to capture exactly "this command's output" ([`docs/features/terminal-output/index.md`](docs/features/terminal-output/index.md))
- **Watch terminal output live and trigger follow-up work** — parses PTY output lines and fans them out to memory/file sinks automatically ([`docs/features/terminal-output/index.md`](docs/features/terminal-output/index.md))
- **Cap agent token spend automatically** — tracks and aggregates usage, auto-blocking once a cost cap is exceeded ([`docs/features/telemetry/index.md`](docs/features/telemetry/index.md))
- **Theme it your way** — a user-customizable theme system built on a 4px grid and a 14px font-size ceiling ([`docs/features/themes/index.md`](docs/features/themes/index.md))
- **Run several child Claude instances and get notified as each finishes** — spawn/tell return immediately, and a completion notification arrives automatically whenever a child goes idle, needs input, or exits ([`docs/plugins/claude/index.md`](docs/plugins/claude/index.md))

## Documentation

- Index: [`docs/index.md`](docs/index.md)
- User guides: [`docs/installation.md`](docs/installation.md), [`docs/features/`](docs/features/index.md)
- Agent guides: [`docs/reference/`](docs/reference/index.md) (api / event-catalog / output-parsers / environments / plan.schema.json)
- Developer guides: [`docs/dev-guide/`](docs/dev-guide/)
- Stability policy: the "Stability Policy" section of [`docs/dev-guide/api-conventions.md`](docs/dev-guide/api-conventions.md)

## Architecture

A hexagonal architecture (model + ports + adapters + view + host_api separation) across a 60-crate workspace. Full structure: [`docs/architecture/`](docs/architecture/).

## License

MIT — [`LICENSE`](LICENSE). Bundled third-party assets and their notices: [`THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md).
