---
name: feature-branch
description: Workflow for adding a new feature to cecilia - creates a worktree branch (via T3 Code, herdr or plain git worktree, whichever the user is using), delegates the work to a bounded subagent, merges main into the branch and tests, then merges into main. Use this whenever the user wants to add, implement, build or prototype a feature, command, setting, pane, tool or format support, even if they don't mention branches or worktrees. Skip it for small bug fixes, renames and docs-only edits unless asked.
---

# Feature branch workflow

Use for every new feature (not for small fixes or docs-only edits unless asked).

## 1. Create the worktree

Always work in a separate worktree, never in the main checkout. Create it with whatever tool
the user is working in or asks for (T3 Code, herdr, or plain `git worktree`). If the user hasn't
said which, ask. Start from an up-to-date `main`, on a branch named `feat/<short-name>`
(kebab-case). Examples:

```sh
# plain git
git worktree add -b feat/<short-name> ../cecilia-<short-name> main
# herdr
herdr worktree create --cwd /home/kjm/Projects/cecilia --branch feat/<short-name> --base main --label <short-name> --no-focus
```

For T3 Code, create the worktree through its own UI/flow with the same branch name and base.

Note the worktree path (and workspace ID, if the tool reports one).

`test-corpus/` is gitignored (and holds symlinks), so a new worktree doesn't have it. Link it
in so the subagent can use the sample scores for tests and screenshots:

```sh
ln -s /home/kjm/Projects/cecilia/test-corpus <worktree>/test-corpus
```

## 2. Implement in a bounded subagent

Spawn one subagent (Agent tool, `general-purpose`) whose working directory is the worktree.
Give it a self-contained brief:

- the feature description and acceptance criteria,
- the worktree path (all edits and commands happen there, never in the main checkout),
- the repo rules from `AGENTS.md`: behaviour in `cecilia-app` with tests, both `cecilia-gui` and
  `cecilia-tui` updated for pane/widget changes, `docs/keymap.md` and `docs/config.example.toml`
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

1. Run `just bump minor` (or `patch`, or an explicit `just bump X.Y.Z`). It sets the version in
   `Cargo.toml` and `Cargo.lock`, moves the `[Unreleased]` entries in `CHANGELOG.md` under a new
   dated heading, and updates the AUR PKGBUILDs and the Scoop, Winget and Chocolatey manifests. It
   refuses an empty `[Unreleased]`.
2. Commit as `chore(release): vX.Y.Z`, then tag that commit: `git tag -a vX.Y.Z -m "vX.Y.Z"`.

State the chosen version and why when reporting. Do not push the commit or tag unless asked.

Afterwards offer to remove the worktree (with the same tool that created it: `git worktree remove
<path>`, `herdr worktree remove --workspace <ID>`, or T3 Code's own cleanup) and delete the
branch. Do not push unless asked.
