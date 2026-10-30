---
name: feature-branch
description: Workflow for adding a new feature to muse - creates a herdr worktree branch, delegates the work to a bounded subagent, merges main into the branch and tests, then merges into main. Use this whenever the user wants to add, implement, build or prototype a feature, command, setting, pane, tool or format support, even if they don't mention branches or worktrees. Skip it for small bug fixes, renames and docs-only edits unless asked.
---

# Feature branch workflow

Use for every new feature (not for small fixes or docs-only edits unless asked).

## 1. Create the worktree

From the main checkout, on an up-to-date `main`:

```sh
herdr worktree create --cwd /home/kjm/Projects/muse --branch feat/<short-name> --base main --label <short-name> --no-focus
```

Note the worktree path and workspace ID it reports. Branch names are kebab-case with a
`feat/` prefix.

## 2. Implement in a bounded subagent

Spawn one subagent (Agent tool, `general-purpose`) whose working directory is the worktree.
Give it a self-contained brief:

- the feature description and acceptance criteria,
- the worktree path (all edits and commands happen there, never in the main checkout),
- the repo rules from `AGENTS.md`: behaviour in `muse-app` with tests, both `muse-gui` and
  `muse-tui` updated for pane/widget changes, `docs/keymap.md` and `docs/config.example.toml`
  kept in sync, a line added under `[Unreleased]` in `CHANGELOG.md` (and the item removed from
  `TODO.md` if it was listed there),
- scope limits: only the files the feature needs, no unrelated refactors, no merging, no
  pushing, no touching `main`,
- to commit on the feature branch with Conventional Commits (`feat:`, `fix(engrave):` …),
- to finish by running `cargo fmt`, `cargo clippy --workspace --all-targets` and
  `cargo test --workspace`, and report what changed and the results.

Review the subagent's report and diff (`git -C <worktree> diff main...HEAD`) before moving on.

## 3. Merge main into the feature branch

In the worktree, before touching `main`:

```sh
git -C <worktree> merge main
```

Resolve conflicts, then rebuild and run `cargo clippy --workspace --all-targets` and
`cargo test --workspace` there. This catches breaking API changes from `main`. Fix anything
that breaks on the feature branch, not on `main`.

## 4. Merge into main

The project rule is to ask the user when to merge. Once tests pass on the feature branch,
report the result and ask for the go-ahead, then from the main checkout:

```sh
git merge --no-ff feat/<short-name>
cargo test --workspace
```

## 5. Bump the version and tag

Right after the merge, on `main`, pick the semantic version bump (the project is pre-1.0, and
the user decides when to reach 1.0.0): a new feature is a minor bump, a fix-only change a patch
bump, and a breaking change a minor bump while on 0.x. Then:

1. Set `version` in the workspace `Cargo.toml` (`[workspace.package]`; the crates inherit it)
   and run `cargo build` so `Cargo.lock` updates.
2. In `CHANGELOG.md`, move the `[Unreleased]` entries under a new `## [x.y.z] - YYYY-MM-DD`
   heading, keeping an empty `[Unreleased]` above it.
3. Commit as `chore(release): vX.Y.Z`, then tag that commit: `git tag -a vX.Y.Z -m "vX.Y.Z"`.

State the chosen version and why when reporting. Do not push the commit or tag unless asked.

Afterwards offer to remove the worktree (`herdr worktree remove --workspace <ID>`) and delete
the branch. Do not push unless asked.
