# LupeHub

Agent-native source control.

LupeHub stores prompt-sized checkpoints and cheap saves inside those checkpoints.
It does not use Git, GitHub, Docker, or an external database as its backend.

## Storage

The CLI starts itself automatically. If it finds a `.lupe` directory in the
current directory or a parent, it uses that project store. If not, it creates
`.lupe` in the current workspace.

- SQLite database: checkpoints, saves, manifests, searchable text.
- Content-addressed object store: file bytes.

Override the location only when needed:

```bash
LUPE_HOME=/path/to/lupe-data lupe status
```

## Primitives

### checkpoint

A checkpoint is the meaningful unit of work. It holds the full user prompt, the
agent's response, and at least one save (the initial workspace snapshot taken
when the checkpoint was created).

```bash
lupe prompt "full user prompt"          # create checkpoint + initial save
lupe prompt "full user prompt" --title "short override"
```

### save

A save is a cheap snapshot of the workspace inside the current checkpoint.
Create them freely — before risky edits, after tests pass, before trying a
different approach. They do not pollute any external history.

```bash
lupe save "what now works or changed"
```

### restore

Restore rolls the workspace back to any saved state. Inspect the target first.
Create a save of the current state if it has anything worth keeping.

```bash
lupe restore <save-uuid>
```

### diff

Compare any two saves. With no arguments, compares the two most recent saves
in the current checkpoint.

```bash
lupe diff
lupe diff <from-save-uuid> <to-save-uuid>
```

### respond

Attach the agent's response text to the latest checkpoint. Called automatically
by the stop hook when hooks are installed.

```bash
lupe respond "full agent response"
```

### push

Stage everything, commit using the latest checkpoint title as the message, and
push to the git remote. Optional — only useful when the workspace is a git repo.

```bash
lupe push
lupe push --message "override commit message"
```

## Read Commands

```bash
lupe status                   # show active database and object store paths
lupe history                  # list checkpoints
lupe prompts                  # list checkpoints with prompts and responses
lupe saves                    # list saves (all, or pass a checkpoint uuid)
lupe graph                    # colored terminal graph of checkpoints and saves
lupe search "query"           # full-text search across titles, prompts, responses
```

## Setup

```bash
lupe install                  # configure this workspace and agent stop hooks
lupe install --workspace PATH # configure another workspace
lupe install-hooks            # only wire stop hooks into Claude Code, Codex, Cursor
lupe install-agent            # only append Lupe workflow instructions to AGENTS.md
```

`lupe install` records the current `lupe` binary path in hook commands with
`LUPE_BIN=...`, so hooks do not depend on a hardcoded user-specific path. The
hook script still falls back to `lupe` on `PATH` and then `~/.cargo/bin/lupe`.

## Agent Instructions

Agents should follow [AGENTS.md](AGENTS.md). Keep it updated whenever Lupe's CLI,
storage, terminology, or workflow changes.

There is also a portable skill draft at [skills/lupe-agent/SKILL.md](skills/lupe-agent/SKILL.md).

To add Lupe instructions to another workspace:

```bash
lupe install --workspace /path/to/workspace
```
