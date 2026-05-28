# Lupe Agent Workflow

Lupe is agent-native source control. It is not Git, and it does not use GitHub,
Docker, Postgres, or a server in the current implementation.

Use Lupe whenever you are acting as an agent in a workspace and may change files.

## Core Concepts

- `prompt` records a user prompt and creates the meaningful checkpoint/task node.
- A `checkpoint` is the meaningful prompt/task node. Prefer `lupe prompt` so the full prompt is always attached.
- A `save` is a cheap source-state snapshot inside the current checkpoint. Create saves freely during work.
- `history` shows checkpoints with prompt snippets.
- `prompts` shows the full prompt history.
- `saves` shows saved states.
- `graph` shows a colored terminal graph of checkpoints and nested saves.
- `diff` with no args compares the latest two saves in the current checkpoint.
- `diff <from> <to>` compares explicit saved states.
- `restore` restores a saved state into the workspace.
- `search` searches remembered checkpoint/save text.

## Required Agent Behavior

The stop hook auto-captures every user message and agent response.
If the host has `lupe install-hooks` configured, `lupe prompt` and `lupe respond`
are called automatically after each turn — no manual calls needed.

During work, still run saves manually:

```bash
lupe save "<short description of what now works or changed>"
```

If hooks are NOT installed (e.g. a plain terminal), call manually:

```bash
lupe prompt "<full user prompt>"
# ... do work ...
lupe respond "<full agent response text>"
```

Save after each coherent functional unit, including:

- after a meaningful edit batch
- before a risky refactor
- before trying a different approach
- after tests pass
- after fixing a failing test/build
- before a restore or other potentially destructive operation

Do not treat Lupe saves like serious public commits. They are intentionally cheap.
The point is to preserve agent working state without polluting Git history.

## Recovery And Recall

Use these when you need context or rollback:

```bash
lupe history
lupe prompts
lupe saves
lupe graph
lupe search "<topic>"
lupe diff
lupe diff <from-save-uuid> <to-save-uuid>
lupe restore <save-uuid>
```

Before using `lupe restore`, inspect the target save and create a new save of the
current state if there is any useful work to preserve.

## Current CLI

```bash
lupe status
lupe prompt "full user prompt"
lupe checkpoint "task summary" --prompt "full user prompt"
lupe save "save summary"
lupe history
lupe prompts
lupe saves
lupe graph
lupe search "query"
lupe diff
lupe diff <save-a-uuid> <save-b-uuid>
lupe restore <save-uuid>
lupe respond "full agent response"
lupe install-agent
```

Storage starts automatically. If Lupe finds `.lupe` in the current directory or
a parent, it uses that project store. If not, it creates `.lupe` in the current
workspace. `lupe status` shows the active database/object-store paths and mode.
Use `LUPE_HOME` or `--home` to override the storage location.

`lupe install-agent` writes or appends Lupe instructions to `AGENTS.md` in the
current workspace.

## Keep This Updated

Whenever Lupe behavior, commands, storage, or terminology change, update this
file in the same change. Agents depend on this file to know how to use Lupe.


<!-- lupe-agent-workflow -->
# Lupe Agent Workflow

Lupe is prompt-driven source control for agents.

At the start of every user request that may modify files, run:

```bash
lupe prompt "<full user prompt>"
```

During work, run:

```bash
lupe save "<short description of what changed or now works>"
```

Save after each coherent functional unit, before risky changes, after tests pass,
and before restore/destructive operations.

Useful commands:

```bash
lupe history
lupe prompts
lupe saves
lupe search "<topic>"
lupe diff <from-save-uuid> <to-save-uuid>
lupe restore <save-uuid>
```

Lupe does not automatically see prompts unless the agent or host calls Lupe.
This file is the contract that tells agents when to call it.
<!-- /lupe-agent-workflow -->
