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
lupe use fix-login-bug                  # route this directory to the branch
lupe branches                           # list all branches
lupe restore fix-login-bug              # restore to a branch's head
lupe use main                           # route back to main
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
lupe graph --web --port 4747  # interactive graph in browser
lupe search "query"           # full-text search across history
lupe files <checkpoint-id>    # list files in a checkpoint
lupe cat <file> <checkpoint>  # print a file as it existed in a checkpoint
```

## Setup

Two steps: wire the stop hooks, then install the agent skills.

```bash
lupe install                  # configure stop hooks (records LUPE_BIN path)
lupe agent-install            # install the Lupe setup skill into Codex, Claude, Cursor
lupe install-skill            # install the Lupe workflow skill into Codex, Claude, Cursor
```

`lupe install` records the current `lupe` binary path in hook commands with
`LUPE_BIN=...`, so hooks do not depend on a hardcoded user-specific path. Use
`lupe install-hooks` to wire only the stop hooks without anything else.

Target a single agent with `--agent`:

```bash
lupe agent-install --agent codex     # also: --agent claude, --agent cursor
lupe install-skill  --agent codex
```

### How agents learn Lupe

Agents pick up Lupe through **skills**, not project files. `agent-install`
installs the setup skill (how to install and start Lupe) and `install-skill`
installs the workflow skill (checkpoints, branches, privacy) into each agent's
skill directory. The agent loads them automatically on its next session.

Lupe **never** creates, edits, or paraphrases instructions into `AGENTS.md`,
`CLAUDE.md`, or any project file. Setup is skills + the `lupe` commands only —
nothing is written into your repo's agent-instruction files. If an agent tries
to copy the workflow into `AGENTS.md`, it is misbehaving: re-run
`lupe install-skill` so it loads the skill instead.

## Privacy

```bash
lupe private                    # flag the next checkpoint as private
lupe prompt --private "prompt"  # create a private prompt checkpoint directly
```

Private checkpoints are hidden from `history`, `graph`, and `prompts` by
default. Add `--show-private` to reveal them.

## Real-time Graph

`lupe graph --web` opens a browser UI that updates in real time as agents
write checkpoints on any branch. Useful for watching parallel agent
orchestration live.

## Merge Workflow (agent-driven)

To merge two branches:

1. `lupe graph --all` — identify the two branch tips
2. `lupe diff <ancestor> <main-tip>` — what main changed
3. `lupe diff <ancestor> <branch-tip>` — what branch changed
4. `lupe files <branch-checkpoint>` — see all files in branch
5. `lupe cat <file> <branch-checkpoint>` — read a specific file from branch
6. Resolve conflicts by writing files to disk
7. `lupe save "merged <branch-name> into main"`
