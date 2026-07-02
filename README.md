# gitm

AI-aware multi-repo workspace orchestrator. Groups several git projects under one "workspace root", runs commands across them in parallel, and auto-maintains a `CLAUDE.md` subproject catalog using an AI backend (claude / opencode / codex).

Single static Rust binary. No Node/Python runtime. Git submodules are **not** used — projects are independent clones/worktrees tracked by a TOML registry.

## Install

```bash
# from source
cargo install --path .

# or build and symlink
cargo build --release
ln -s "$PWD/target/release/gitm" ~/.local/bin/gitm
```

Prebuilt binaries are published as GitHub Release assets (see [Update](#update) below):

```bash
# once a release exists:
curl -fsSL https://github.com/reven404/gitm/releases/latest/download/... | tar xz
```

## Quick start

```bash
gitm init myws            # create workspace, detect AI backend, scan existing sub-repos
cd myws
gitm add git@github.com:org/service.git      # clone (branch = workspace name "myws")
gitm add ../local-repo --tag backend          # worktree of a local repo
gitm ls                                      # live status: branch / dirty / ahead / behind
gitm ls --format json
gitm x -j 4 -- 'make test'                   # run across all projects in parallel
gitm x -t backend -- 'go build ./...'        # filter by tag
gitm x --dry-run -- 'make test'              # preview
gitm sync                                    # fetch + ff-only pull, skip dirty
gitm pr create                               # delegate to gh/glab per repo
gitm rm service                              # unregister (+ worktree remove)
```

The root `CLAUDE.md` gets a **Subproject Catalog** table row per project, filled by the AI backend:

```
| service | Node 18 + Egg.js | 表单后端服务 |
```

## Commands

| Command | Description |
|---|---|
| `gitm init [DIR] [--ai <b>] [--no-scan]` | Init workspace, git init, write `CLAUDE.md`, detect AI backend, scan existing git sub-repos. |
| `gitm add <SRC> [NAME] [--tag <T>]...` | `git clone` (URL) or `git worktree add` (local path); branch = workspace name; writes toml + AI row. |
| `gitm ls [--format table\|json] [--tag <T>] [--watch [S]]` | Live status per project. `--watch` polls (default 2s). |
| `gitm x [-p NAME] [-t <T>] [-j N] [--fail-fast] [--dry-run] -- <CMD>` | Run a shell command across projects in parallel. `--` separates. |
| `gitm sync [-j N]` | `git fetch --prune` + `git pull --ff-only`; skips dirty repos. |
| `gitm pr <create\|list\|view> [-p NAME] [-t <T>]` | Delegate to `gh`/`glab` based on each repo's remote host. |
| `gitm rm <NAME> [--force]` | Unregister; worktree-removes worktrees, deletes cloned with `--force`, keeps local. |
| `gitm ai [NAME] [--refresh]` | Re-run AI analysis, rewrite `CLAUDE.md` rows. |
| `gitm version` | Print version / target / repo / detected AI backends. |
| `gitm update [--check]` | Check GitHub for a newer release; download + self-replace. |
| `gitm docs` | Print the AI-friendly reference (same as `llms.txt`). |

Global flags (placed before the subcommand): `--no-ai`, `--ai <backend>`, `--no-check`, `-v`.

### `x` semantics

Everything after `--` is joined into one string and run via `sh -c` in each project dir, so shell features work:

```bash
gitm x -j 4 -- 'go test ./... && go build'
gitm x -- 'echo $(basename "$PWD")'
```

Do **not** wrap it as `gitm x -- sh -c '...'` (that nests shells). Pass the script string directly.

## Configuration: `gitm.toml`

```toml
[workspace]
branch = "myws"          # default branch for all sub-projects (= init dir basename)

[ai]
backend = "claude"       # claude | opencode | codex | none

[[project]]
name = "service"
source = "git@github.com:org/service.git"
type = "cloned"           # cloned | worktree | local
path = "service"
branch = "myws"           # optional override of [workspace].branch
tags = ["backend"]
```

`find_root()` walks up from cwd to locate `gitm.toml`, so commands work from any subdirectory of the workspace.

## AI backend

At `init`, gitm probes PATH for `claude`, `opencode`, `codex` (via `--version`). If multiple are found it prompts you to pick one; `--ai <backend>` skips the prompt. Non-interactive (no TTY) falls back to the first detected. Override anytime with `--ai` or by editing `[ai].backend`.

Each backend is invoked non-interactively with the project dir as cwd so it can read manifest files:

| Backend | Invocation |
|---|---|
| claude | `claude -p "<prompt>"` |
| opencode | `opencode run "<prompt>"` |
| codex | `codex exec "<prompt>"` |

AI is best-effort: on failure or `--no-ai`, a `| <name> | (pending) | |` placeholder row is written and can be filled later with `gitm ai --refresh <name>`.

## Update

gitm checks GitHub for a newer release on startup, throttled to once per 24h (cached at `~/.cache/gitm/last_check`). If a newer version exists it prints a one-line notice; the check is silent on network failure and never blocks.

```bash
gitm update --check    # show current vs latest
gitm update            # download matching asset, extract, replace the running binary
```

Repo is `reven404/gitm` by default; override at build time with `GITM_REPO_OWNER` / `GITM_REPO_NAME` env vars, or at runtime. Release assets are named `gitm-<arch>-<os>.tar.gz` (e.g. `gitm-aarch64-darwin.tar.gz`) and produced by `.github/workflows/release.yml` on `v*` tag push.

`--no-check` skips the startup check (CI / offline).

## Status cell legend (`ls`)

| Marker | Meaning |
|---|---|
| `✔` | clean, in sync with upstream |
| `✱dirty` | uncommitted changes |
| `↑N` | N commits ahead of upstream |
| `↓N` | N commits behind upstream |
| `∅no-upstream` | no tracking branch |

## Scope (non-goals)

- Fleet-level PR lifecycle (merge/close/status across an org) — that's [multi-gitter](https://github.com/lindell/multi-gitter).
- Manifest version pinning (commit SHAs) — that's Google's `repo`.
- Recursive/nested workspaces, Windows support (`sh -c`), self-hosted Git API beyond `gh`/`glab`.

## License

MIT
