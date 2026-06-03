# LupeSource

Agent-native source control.

Lupe stores prompt-sized checkpoints in a local SQLite store. Every agent session is a checkpoint. Branches let multiple agents work in parallel and stay visible in real time.

It does not use Git, GitHub, Docker, or an external database as its backend.

## Storage

Lupe is **opt-in per project**. Run `lupe init` once in a project to set it up.
This creates `.lupe/`, writes `.lupeignore`, and adds `.lupe/` to `.gitignore`
if a git repo is present. The hook then activates automatically for that project.

If a `.lupe` directory already exists in the current directory or a parent
(but not beyond `$HOME`), lupe uses that project store.

- SQLite database: checkpoints, branches, file manifests, searchable text.
- Content-addressed object store: file bytes.

Override the location only when needed:

```bash
LUPE_HOME=/path/to/lupe-data lupe status
```

## Primitives

### checkpoint

A checkpoint is a full snapshot of the workspace. It holds the user prompt, the
agent's response, and the file state at that moment.

```bash
lupe prompt "full user prompt"          # record prompt + snapshot
lupe save "what changed"                # snapshot mid-task
```

### branch

A branch is a named pointer to a checkpoint tip. Every checkpoint belongs to a
branch. The default branch is `main`. Workspaces auto-route to their branch via
a `.lupe-branch` file — no env vars, no config.

```bash
lupe branch "fix-login-bug"             # create branch at current HEAD
lupe branches                           # list all branches
lupe restore fix-login-bug              # restore to a branch's head
```

### workspace

A workspace is an isolated directory for parallel agent work. Creating one
branches from the current HEAD and writes `.lupe-branch` so all checkpoints
inside it route to the correct branch automatically.

```bash
lupe workspace new my-feature           # create workspace, branch from HEAD
lupe workspace list
lupe workspace drop my-feature
```

### restore

Roll the workspace back to any checkpoint or branch head. Never edit files
manually to revert — use restore.

```bash
lupe restore <checkpoint-uuid>
lupe restore <branch-name>
```

### diff

Compare any two checkpoints.

```bash
lupe diff                               # last two checkpoints
lupe diff <from-uuid> <to-uuid>
```

## Read Commands

```bash
lupe status / lupe init       # show active store, setup if needed
lupe history                  # list checkpoints on main branch
lupe history --all            # list all branches
lupe prompts                  # list checkpoints with prompts
lupe graph                    # terminal graph of main branch
lupe graph --all              # terminal graph of all branches
lupe graph --web              # interactive graph in browser
lupe search "query"           # full-text search across history
lupe files <checkpoint-id>    # list files in a checkpoint
lupe cat <file> <checkpoint>  # print a file as it existed in a checkpoint
```

## Setup

```bash
lupe install                  # configure workspace + agent stop hooks
lupe install --workspace PATH # configure another workspace
lupe install-hooks            # only wire stop hooks (Claude Code, Codex, Cursor)
lupe install-agent            # only write AGENTS.md workflow instructions
```

`lupe install` records the current `lupe` binary path in hook commands with
`LUPE_BIN=...`, so hooks do not depend on a hardcoded user-specific path.

## Real-time Graph

`lupe graph --web` opens a browser UI that updates in real time as agents
write checkpoints on any branch. Useful for watching parallel agent
orchestration live.

## Agent Instructions

Agents should follow [AGENTS.md](AGENTS.md). Keep it updated whenever Lupe's
CLI, storage, terminology, or workflow changes.

To add Lupe instructions to another workspace:

```bash
lupe install --workspace /path/to/workspace
```

## Merge Workflow (agent-driven)

To merge two branches:

1. `lupe graph --all` — identify the two branch tips
2. `lupe diff <ancestor> <main-tip>` — what main changed
3. `lupe diff <ancestor> <branch-tip>` — what branch changed
4. `lupe files <branch-checkpoint>` — see all files in branch
5. `lupe cat <file> <branch-checkpoint>` — read a specific file from branch
6. Resolve conflicts by writing files to disk
7. `lupe save "merged <branch-name> into main"`
