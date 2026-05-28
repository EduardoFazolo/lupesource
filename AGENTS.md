# Lupe Agent Workflow
<!-- test comment to trigger a diff — second edit -->

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

## Project Setup — Do This First

At the start of any session in a new project, check for `.lupeignore` and
`.lupeshared`. If either is missing, detect the stack and create them.

**Detect stack by looking for:**
- `package.json` → Node/JS/TS
- `Cargo.toml` → Rust
- `requirements.txt` / `pyproject.toml` / `uv.lock` → Python
- `go.mod` → Go
- `pom.xml` / `build.gradle` → Java/Kotlin
- Multiple of the above → monorepo, include all

**`.lupeignore`** — what lupe will NOT snapshot (created automatically on first
`lupe prompt`, but you can create it earlier with better stack-specific content):

```
# Node
node_modules
dist
.next
.nuxt
out

# Rust
target

# Python
.venv
__pycache__
*.pyc
.pytest_cache

# General
.git
.lupe
build
dist
```

**`.lupeshared`** — what gets symlinked (not copied) into `lupe workspace` forks.
Should list anything expensive to reinstall (dependencies, build artifacts):

```
# Node
node_modules

# Rust
target

# Python
.venv
```

Only include entries that actually exist in the project. Ask the user to confirm
before writing if you are unsure about the stack.

## When to Create a Workspace

Create a workspace proactively — without waiting for the user to ask — when:

- User wants to try two different approaches to the same problem
- User says "test this in isolation", "try this separately", "don't break what's working"
- You are about to make large, risky, or experimental changes to files the user is actively using
- User asks you to implement a feature while another agent is working on the same codebase
- You are exploring an approach you are not confident about

Workflow:
1. `lupe save "stable state before experiment"` — snapshot current state
2. `lupe workspace new "<descriptive-name>"` — create isolated workspace
3. Tell the user the path so they can open it / run the app there
4. Do the experimental work inside the workspace
5. If it works: user promotes it. If not: `lupe workspace drop <name>`, no damage done.

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

**Never revert or undo work by manually editing or deleting files.** When dropping
a feature, rolling back a change, or abandoning an approach, always use
`lupe restore` to move HEAD to the pre-feature save. This preserves the dropped
work as a dead branch visible in `lupe graph`.

Workflow for dropping a feature:
1. `lupe save "feature complete before drop"` — preserve the current state
2. `lupe saves` — find the save taken before the feature was started
3. `lupe restore <pre-feature-save-uuid>` — move HEAD back

Before using `lupe restore`, inspect the target save and create a new save of the
current state if there is any useful work to preserve.

## Current CLI

```bash
lupe status
lupe install
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
lupe install-hooks
lupe author
lupe author --name "Your Name" --email "your@email.com"
lupe workspace new <name>
lupe workspace list
lupe workspace drop <name>
```

Storage starts automatically. If Lupe finds `.lupe` in the current directory or
a parent, it uses that project store. If not, it creates `.lupe` in the current
workspace. `lupe status` shows the active database/object-store paths and mode.
Use `LUPE_HOME` or `--home` to override the storage location.

`lupe install` configures the current workspace and wires agent stop hooks.
`lupe author` reads the current author name and email for this project store.
`lupe author --name X --email Y` sets them (both optional; partial updates OK).
If author is not configured when starting a session, ask the user for name and
email and set them with `lupe author --name "..." --email "..."`.

`lupe install-agent` writes or appends Lupe instructions to `AGENTS.md` in the
current workspace. `lupe install-hooks` only wires stop hooks.

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
lupe install
lupe history
lupe prompts
lupe saves
lupe search "<topic>"
lupe diff <from-save-uuid> <to-save-uuid>
lupe restore <save-uuid>
lupe install-agent
lupe install-hooks
```

Lupe does not automatically see prompts unless the agent or host calls Lupe.
This file is the contract that tells agents when to call it.
<!-- /lupe-agent-workflow -->
