# Drink development instructions

This project follows the Rust template for its scheduling core.
Keep these instructions aligned with the README, hooks, build configuration and CI.

## Working Policy

For Drink, fetch from `origin` before starting work and incorporate upstream
changes while preserving local edits. Push each verified, signed commit to the
branch it tracks on `Napolitain/drink`. Confirm the remote branch tip matches
the local commit before reporting the work synced.

Read `README.md`, the relevant source, build configuration, hooks and CI before
editing. Inspect the branch and worktree; preserve unrelated user changes and
keep the work focused on the requested outcome. Finish implementation,
verification and documentation together.

Prefer small, clear changes that fit the existing design. Keep entry points thin,
put reusable behavior in testable modules, and refactor excessive complexity
instead of suppressing checks. Do not weaken warnings, type checks, safety rules,
coverage thresholds or tests to make a change pass. Review every autofix,
especially fixes that can change behavior.

Use the latest stable compatible languages, runtimes, tools and dependencies when
selecting or upgrading them. Verify choices against current primary sources and
document compatibility constraints. Respect the existing pinned environment for
routine work; keep dependency upgrades intentional and include the relevant lock
files. Clarify consequential library choices or missing constraints with the user
before adopting them.

## Verification Policy

Use the repository's existing build, lint and test commands. Add meaningful
regression tests for bug fixes and cover new behavior, boundaries and failure
paths. Prefer observable behavior over tests that duplicate implementation.
Use the existing property and mutation testing tools where they help validate
invariants or expose weak assertions.

Run focused checks while iterating. Before committing code, run the affected
template's pre-commit and pre-push gates and the relevant build, including the
release build when compilation, packaging or performance is affected. Review
autofixes and rerun affected checks after the final edit. Run configured manual
mutation checks when changing critical logic or assessing test quality.

For documentation-only changes, verify commands and paths against the actual
configuration, check formatting and links, and run hooks on the changed files.
Hooks configured to always run still apply. Do not add artificial tests or run
unrelated builds merely to validate prose. Changes to executable examples,
configuration or build instructions may require the corresponding commands.

All required checks must pass before committing. If a required check cannot run,
report the blocker and leave the affected work uncommitted unless the user
explicitly accepts that verification gap. Record exact commands, results and
limitations; distinguish static review, local tests, builds, CI and runtime proof.
Never report an unrun check as passing.

## Performance Policy

Treat performance as part of design, alongside correctness and maintainability.
Consider algorithmic cost, allocations, data layout, I/O, concurrency, dependency
weight and build cost. Prefer efficient supported options, but justify tradeoffs
with evidence and avoid speculative complexity.

Before optimization, establish the workload, baseline and success metric. Clarify
missing latency, throughput, memory, startup or resource constraints with the user
when they affect the approach. Profile to identify the bottleneck, then compare
baseline and candidate on representative inputs using the same toolchain,
dependencies, flags and environment.

Use optimized production builds for performance measurements and test their
correctness. Keep debug, coverage and instrumentation overhead out of the final
comparison. Use repeated runs and appropriate warmups; record variability, relevant
tail latency, peak memory, output size and build cost. Preserve reproducible
commands and results, and report regressions as well as gains. A greeting or
pipeline smoke test is not evidence of an application speedup.

Keep profile-guided optimization optional. Separate training inputs from evaluation
inputs, regenerate profiles after relevant source/toolchain/workload changes, and
check the final optimized artifact. Use only modes supported by this template.
Retain the baseline when gains are within noise or violate agreed constraints.
Respect host resource limits and run competing benchmarks or profiling builds
serially.

## Commit Message Policy

Create git commits automatically as work reaches coherent checkpoints unless the
user explicitly asks not to commit, the worktree contains user-owned changes that
cannot be safely separated, or verification has not reached a reasonable commit
point. Keep commits as granular as practical: one commit per independently
reviewable feature, fix, test, documentation update, refactor, performance change,
build change, CI change, chore or revert. Do not mix unrelated concerns to reduce
commit count, or split a behavior change from the tests, documentation and
generated artifacts needed to understand and verify it. Every commit must pass
the required checks and tests described above before it is made.

Every commit must use a Conventional Commits subject and a highly descriptive
body. Use the type matching the change: `feat`, `fix`, `test`, `docs`,
`refactor`, `perf`, `build`, `ci`, `chore` or `revert`. The body is mandatory:
commit descriptions are the durable audit trail and are used for internal logging.

Every commit body and pull request description must use the following four
labels exactly as written, in this order. Start each section on a new line and
fill it with details specific to the change.

```text
What: Describe what changed in this commit or pull request.

Why: Explain the problem or need that motivated the change.

How: Explain how the change was implemented.

Result: State the observed outcome. Include before-and-after behavior or
verification results where relevant, and identify anything still unverified.
```

Use specific evidence such as affected options, issue numbers and generated files.
Avoid vague subjects such as `fix`, `update` or `changes`, bodies that merely
repeat the subject, and unsupported verification claims. A future reader must be
able to understand the task, motivation and implementation from the commit alone.

## Build and Test Commands

Run commands from the repository root using the toolchain in `rust-toolchain.toml`.

```sh
prek run -a
prek run -a --hook-stage pre-push
cargo build --release
linux/build  # on Linux with the Plasma Wayland development dependencies in README.md
linux/tests/plasma-session  # isolated two-output KWin and PipeWire smoke test
macos/build  # on macOS with Apple Command Line Tools
windows/build.ps1  # on Windows with the Visual Studio C++ build tools and PowerShell 7.3+
```

Use `cargo test --all-targets --all-features` for focused iteration. Hooks run
rustfmt, Clippy autofixes, Clippy with warnings denied, and cargo-llvm-cov with
all targets/features and an 80% line coverage gate across the workspace except
the CXX bridge. The Windows adapter's Win32 modules compile, and are measured,
only on Windows. Keep entry points thin: `src/main.rs` and `src/bin/` are
excluded from the coverage gate in every package. Preserve the
unsafe-code prohibition, enabled lint groups and cognitive complexity limit of 15.
Commit proptest regression cases so failures are replayed.

Run `prek run --hook-stage manual cargo-mutants --all-files` for mutation testing
when applicable; inspect survivors rather than relying only on coverage.

## Template Performance Guidance

Retain the release profile's fat LTO, single codegen unit, strip and panic settings
unless a measured requirement justifies changing them. Build for the architecture's
portable baseline. The shared core forbids unsafe Rust. The macOS and Plasma
adapters use the same CXX bridge. Keep shared Linux audio observation independent
of the desktop toolkit and Plasma integration in `linux/kde/`. Build and sign-check
the Mac app bundle after changing that interface or the Objective-C++ adapter. The Windows adapter
denies unsafe code except in its Win32 modules, where every block carries a
`SAFETY` comment, and links the C runtime statically. Run
`windows/build.ps1 -SmokeTest` after changing its windows, focus or timer
handling.
