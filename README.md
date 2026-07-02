# gitm

> One workspace, many repos, one AI-maintained `CLAUDE.md`.

`gitm` is an AI-aware multi-repo workspace orchestrator written in Rust. It groups several git projects under one workspace root, runs commands across them in parallel, syncs and opens PRs via `gh`/`glab`, and uses an AI backend (**claude / opencode / codex**) to auto-maintain a `CLAUDE.md` subproject catalog — so a multi-repo workspace becomes a ready-to-use context for AI coding agents.

- **Single static binary.** No Node/Python runtime, no git submodules — projects are independent clones/worktrees tracked by a TOML registry.
- **Parallel by default.** `gitm x -j N -- <cmd>` runs a shell command across all repos in parallel.
- **AI-maintained docs.** On `add`, an AI backend reads each project's manifest and appends a `| name | stack | role |` row to `CLAUDE.md`.
- **Self-updating.** Checks GitHub for new releases (throttled) and `gitm update` replaces the binary in place.
- **AI-friendly.** `gitm docs` prints a token-efficient reference (`llms.txt`); `ls --format json` and friends give machine-readable output.

---

## Install

### Prebuilt binary (recommended)

A one-liner that auto-detects your arch/OS and installs the latest release:

```bash
ARCH=$(uname -m | sed 's/arm64/aarch64/')        # aarch64 | x86_64
OS=$(uname -s | tr '[:upper:]' '[:lower:]')      # darwin  | linux
curl -fsSL "https://github.com/reven404/gitm/releases/latest/download/gitm-${ARCH}-${OS}.tar.gz" \
  | sudo tar xz -C /usr/local/bin gitm
gitm version
```

No `sudo` / no write access to `/usr/local/bin`? Install to a user dir:

```bash
mkdir -p ~/.local/bin
curl -fsSL "https://github.com/reven404/gitm/releases/latest/download/gitm-${ARCH}-${OS}.tar.gz" \
  | tar xz -C ~/.local/bin gitm
export PATH="$HOME/.local/bin:$PATH"   # add to ~/.zshrc / ~/.bashrc to persist
gitm version
```

Verify the download with the SHA-256 digest published on the [release page](https://github.com/reven404/gitm/releases/latest):

```bash
curl -fsSL "https://github.com/reven404/gitm/releases/latest/download/gitm-${ARCH}-${OS}.tar.gz" -o gitm.tgz
shasum -a 256 gitm.tgz   # compare with the digest shown on the release page
tar xzf gitm.tgz -C /usr/local/bin gitm
```

Prebuilt assets (per release):

| Asset | Host |
|---|---|
| `gitm-aarch64-darwin.tar.gz` | Apple silicon (macOS) |
| `gitm-x86_64-darwin.tar.gz` | Intel (macOS) — cross-compiled on Apple-silicon runner |
| `gitm-aarch64-linux.tar.gz` | arm64 (Linux, e.g. Graviton/Raspberry Pi) |
| `gitm-x86_64-linux.tar.gz` | x86_64 (Linux) |

### cargo (from git)

```bash
cargo install --git https://github.com/reven404/gitm --locked
```

### From source

```bash
git clone https://github.com/reven404/gitm
cd gitm
cargo build --release
# binary: target/release/gitm
```

### Upgrade

Once installed, `gitm` self-updates from the same GitHub releases:

```bash
gitm update --check   # show current vs latest
gitm update           # download + extract + replace the running binary
```

---

## Quick start

```bash
gitm init myws                                    # create workspace, detect AI backend, scan sub-repos
cd myws
gitm add git@github.com:org/service.git --tag backend   # clone (branch = "myws")
gitm add ../local-repo --tag frontend                   # worktree of a local repo
gitm ls                                                 # live status: branch / dirty / ahead / behind
gitm ls --format json
gitm x -j 4 -- 'make test'                              # run across all projects in parallel
gitm x -t backend -- 'go build ./...'                   # filter by tag
gitm x --dry-run -- 'make test'                         # preview
gitm sync                                               # fetch + ff-only pull, skip dirty
gitm rm service                                         # unregister (+ worktree remove)
```

The root `CLAUDE.md` gets a **Subproject Catalog** row per project, filled by the AI backend:

```
| service | Node 18 + Egg.js | 表单后端服务 |
```

---

## Commands

| Command | Description |
|---|---|
| `gitm init [DIR] [--ai <b>] [--no-scan]` | Init workspace, `git init`, write `CLAUDE.md`, detect AI backend, scan existing git sub-repos. |
| `gitm add <SRC> [NAME] [--tag <T>]...` | `git clone` (URL) or `git worktree add` (local path); branch = workspace name; writes toml + AI row. |
| `gitm ls [--format table\|json] [--tag <T>]` | Live status per project. |
| `gitm x [-p NAME] [-t <T>] [-j N] [--fail-fast] [--dry-run] -- <CMD>` | Run a shell command across projects in parallel. `--` separates. |
| `gitm sync [-j N]` | `git fetch --prune` + `git pull --ff-only`; skips dirty repos. |
| `gitm rm <NAME> [--force]` | Unregister; worktree-removes worktrees, deletes cloned with `--force`, keeps local. |
| `gitm ai [NAME] [--refresh]` | Re-run AI analysis, rewrite `CLAUDE.md` rows. |
| `gitm version` | Print version / target / repo / detected AI backends. |
| `gitm update [--check]` | Check GitHub for a newer release; download + self-replace. |
| `gitm docs` | Print the AI-friendly reference (same as `llms.txt`). |

Global flags (placed **before** the subcommand): `--no-ai`, `--ai <backend>`, `--no-check`, `-v`.

### `x` semantics

Simple commands need **no quotes** — gitm execs the program directly:

```bash
gitm x -j 4 -- go build ./...
gitm x -t backend -- npm run test
gitm x -- git status -sb
```

If any argument contains shell control chars (`&&`, `|`, `;`, `$`, globs, `>`, ...), gitm runs it through `sh -c` instead — so quote the whole script to keep it as one argument:

```bash
gitm x -- 'make test && make build'
gitm x -- 'echo $(basename "$PWD")'
```

The `--` separator is required so gitm doesn't confuse your command's flags with its own.

---

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

---

## AI backend

At `init`, gitm probes PATH for `claude`, `opencode`, `codex` (via `--version`). If multiple are found it prompts you to pick one; `--ai <backend>` skips the prompt. Non-interactive (no TTY) falls back to the first detected. Override anytime with `--ai` or by editing `[ai].backend`.

Each backend is invoked non-interactively with the project dir as cwd so it can read manifest files:

| Backend | Invocation |
|---|---|
| claude | `claude -p "<prompt>"` |
| opencode | `opencode run "<prompt>"` |
| codex | `codex exec "<prompt>"` |

AI is best-effort: on failure or `--no-ai`, a `| <name> | (pending) | |` placeholder row is written and can be filled later with `gitm ai --refresh <name>`.

---

## Update

gitm checks GitHub for a newer release on startup, throttled to once per 24h (cached at `~/.cache/gitm/last_check`). If a newer version exists it prints a one-line notice; the check is silent on network failure and never blocks.

```bash
gitm update --check    # show current vs latest
gitm update            # download matching asset, extract, replace the running binary
```

Repo is `reven404/gitm` by default; override at build time with `GITM_REPO_OWNER` / `GITM_REPO_NAME` env vars. Release assets are named `gitm-<arch>-<os>.tar.gz` and produced by `.github/workflows/release.yml` on `v*` tag push. `--no-check` skips the startup check (CI / offline).

---

## `ls` status legend

| Marker | Meaning |
|---|---|
| `✔` | clean, in sync with upstream |
| `✱dirty` | uncommitted changes |
| `↑N` | N commits ahead of upstream |
| `↓N` | N commits behind upstream |
| `∅no-upstream` | no tracking branch |

---

## How it compares

| | gitm | [meta](https://github.com/mateodelnorte/meta) | [mani](https://github.com/alajaji/mani) | [multi-gitter](https://github.com/lindell/multi-gitter) |
|---|---|---|---|---|
| Language | Rust | Node.js | Rust | Go |
| Registry | TOML | `.meta` JSON | TOML/YAML | — |
| Parallel exec | ✅ | ❌ (passthrough) | ✅ | ✅ |
| Tags/groups | ✅ | ❌ | ✅ | — |
| Auto `CLAUDE.md` via AI | ✅ | ❌ | ❌ | ❌ |
| Self-update | ✅ | ❌ | ❌ | — |
| Fleet PR lifecycle | ❌ | ❌ | ❌ | ✅ |

gitm's differentiation is the **AI-maintained `CLAUDE.md`**; for fleet-level mass PRs across an org use multi-gitter.

## Scope (non-goals)

- Fleet-level PR lifecycle (merge/close/status across an org) — [multi-gitter](https://github.com/lindell/multi-gitter).
- Manifest version pinning (commit SHAs) — Google's `repo`.
- Recursive/nested workspaces, Windows support (`sh -c`), self-hosted Git API beyond `gh`/`glab`.

## License

MIT
