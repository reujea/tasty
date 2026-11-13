# Contributing to Tasty

Thank you for your interest in Tasty. This guide lists what a pull request needs before it is reviewed. Most project documents are in Korean. Start at [`docs/index.md`](docs/index.md).

## Before you start

- Read [Identity](docs/identity.md) and the [product principles](docs/principles/index.md). A feature that does not fit them is declined, however well it is built.
- For a large feature, or anything that brings in a new tool, runtime or environment, open an issue first and describe what you want to change and why.

## What a pull request must be

### One feature, one pull request

- Put one feature in one pull request. If a feature needs several steps, put all the steps in the same pull request.
- Do not split one feature into ordered pull requests where a later one completes, changes or extends what an earlier one adds. The later pull requests are closed, and you are asked to submit the feature as one pull request.
- A pull request may build on a different feature from another pull request. Say which pull request it depends on at the top of the description.

### Stay within Tasty's stack and environments

- Tasty is a Rust application built on winit, wgpu, egui, termwiz and portable-pty, and it runs on Windows, macOS and Linux. The stack table is in [`CLAUDE.md`](CLAUDE.md).
- A change that depends on a new runtime, external tool or environment (a specific virtual machine, cloud or service) is declined unless it was agreed in an issue first.
- Do not rebuild what an existing feature already does, and do not introduce a new convention the project has not decided on. A new convention brings the work of enforcing it and handling the cases that break it.

### Branches and commits

- Base your pull request on `develop` and keep it rebased on `develop`. See [Branching](docs/dev-guide/branching.md).
- Follow [Conventional Commits](docs/dev-guide/commit-convention.md): `type(scope): description`. Write the subject and body in plain English without Markdown. Put one change in one commit.

## Checklist

Reviewers check each item. Check them yourself before you open the pull request.

- **Identity and principles**: the feature fits [Identity](docs/identity.md) and the [product principles](docs/principles/index.md).
- **Design tokens**: colors, font sizes, line widths and spacing come from `Theme` tokens, never raw pixel values or `from_rgb`. See [Theme rules](docs/design/systems/theme.md).
- **Internationalization**: every UI string goes through `t()`, and every new key is in all four files `lang/{en,ko,ja,zh-Hans}.toml`. See [i18n](docs/dev-guide/i18n.md).
- **Shortcuts**: every new Tasty shortcut is exposed through `KeybindingSettings` so users can change it. See [Key mapping](docs/design/policies/key-mapping.md).
- **Length types**: lengths use `PhysicalPx` and `LogicalPx`, not bare `f32`, with explicit `to_physical` and `to_logical` conversions. See [Typed length](docs/concepts/typed-length.md).
- **Platforms**: OS-specific code is behind `cfg` and does not break the build or behavior on the other operating systems.
- **Focus**: release builds never take focus away from the user, and targets are addressed by ID rather than by the current selection. Debug-only handlers live in separate modules declared with `cfg(debug_assertions)`. See the [focus policy](docs/design/policies/focus.md) and [debug IPC](docs/dev-guide/debug-ipc.md).
- **IPC and CLI**: a feature that agents use is available through both IPC and the CLI.
- **Gallery**: a new UI component is also added to `crates/tasty-gallery`, and popups use `PopupDef`. See [Gallery first](docs/dev-guide/gallery-first.md) and [Popup implementation](docs/dev-guide/popup-implementation.md).
- **Documentation**: new or changed behavior is documented under `docs/`, and the matching index is updated. Documents describe the current state only: no change diary, no references to commits that are not in this repository, and no claims about fixes that are not in the pull request. See the [documentation model](docs/documentation-model.md).
- **Plugin versions**: if a bundled plugin's product content changes, its patch version, manifest and `Cargo.lock` change in the same commit.
- **Security boundary**: a plugin cannot do more than its permissions allow. Permission checks come before side effects, and ownership is decided by the surface owner. See [Plugin permissions](docs/dev-guide/plugin-permissions.md).
- **Resource limits**: sizes, counts, queues, threads and buffers that plugins or external input can grow have a limit.
- **Blocking**: no synchronous wait blocks other work. A request handler that waits on another plugin stalls all IPC, and the UI thread must not wait on I/O, processes or other threads.
- **Input safety**: external input cannot cause a panic, secrets such as keys, environment values and URLs are not logged, and files that hold keys are written with mode 0600.
- **Tests**: new behavior and fixed bugs come with tests, and the tests fail without the change.

## Test locally before you open a pull request

Run the tests the CI workflows run, and open the pull request only after they pass. The CI runners do not run pull requests from first-time contributors until a maintainer approves them, so your local run is the first check your change gets.

1. Install the Git hooks once: `./scripts/dev-setup.sh`. They run part of the checks at commit and push time, not all of them.
2. Run the commands of the workflows that are triggered by pull requests, on your operating system. They are in [`.github/workflows/`](.github/workflows/), and the [CI guide](docs/dev-guide/ci-gates.md) explains which workflow runs what. This guide does not copy the commands, so the workflow files are always the reference.
3. In the pull request description, list the commands you ran, the operating system, and the results. If a test also fails on a clean `develop` checkout, name it and say so. Do not report it as caused by your change, and do not leave it out.

## During review

- Answer every review item, either by changing the code or by replying with your reason. Rebasing alone does not address a review comment.
- When you push changes, say in a comment which review items they address.
