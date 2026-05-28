# Lupe Agent Workflow

Use this skill when working in a repository that uses LupeHub/Lupe for
agent-native source control, or when the user asks you to save/checkpoint agent
work with Lupe.

Lupe is source control for agents:

- `prompt` records a user prompt and creates the meaningful checkpoint/task node.
- `save` creates a cheap source-state snapshot inside the current checkpoint.
- `fork` creates a named pointer to a save — a named branch point you can restore to by name.
- Git/GitHub are not Lupe's backend.

## Project Setup — Do This First

At the start of any session in a new project, check for `.lupeignore`. If missing,
detect the stack and create it.

Detect stack: `package.json` → Node, `Cargo.toml` → Rust, `requirements.txt`/
`pyproject.toml` → Python, `go.mod` → Go, `pom.xml`/`build.gradle` → Java.

`.lupeignore` — what lupe does NOT snapshot. Auto-created on first `lupe prompt`
with generic defaults, but create it early with stack-specific entries
(e.g. `dist`, `.next`, `__pycache__`, `build`).

## Forks — Named Branch Points — MANDATORY

**Before modifying any file, you MUST run `lupe fork "<task-name>"` first. No exceptions.**

```bash
lupe fork "fix-login-bug"         # ALWAYS do this before touching files
lupe forks                        # list all named forks
lupe restore fix-login-bug        # restore by name
```

Do NOT skip this step even for "small" changes.

Trying an alternative approach:
1. `lupe fork "<task-name>"` — FIRST, before any file changes
2. Make the change
3. `lupe save "what changed"`
4. If it works: keep going. If not: `lupe restore <fork-name>` → dead branch in graph.

## Workflow

At the start of every user request that may modify files, run:

```bash
lupe prompt "<full user prompt>"
```

During implementation, run:

```bash
lupe save "<short description>"
```

Save after each coherent functional unit:

- after a meaningful edit batch
- before a risky refactor
- before trying another approach
- after tests pass
- after fixing a failure
- before restore/destructive operations

**Never revert work by editing files manually. Always use `lupe restore`.**

## Recovery And Recall

```bash
lupe history
lupe prompts
lupe saves
lupe forks
lupe graph
lupe search "<topic>"
lupe diff
lupe diff <save-uuid>
lupe diff <from-uuid> <to-uuid>
lupe restore <save-uuid-or-fork-name>
```

## Current Commands

```bash
lupe status
lupe prompt "full user prompt"
lupe save "save summary"
lupe fork "name"
lupe forks
lupe history
lupe prompts
lupe saves
lupe graph
lupe search "query"
lupe diff
lupe diff <save-uuid>
lupe diff <save-a-uuid> <save-b-uuid>
lupe restore <save-uuid-or-fork-name>
lupe respond "full agent response"
lupe author
lupe author --name "Name" --email "email"
lupe install-agent
lupe install-hooks
```

## Maintenance

When Lupe changes, update this skill in the same change.
