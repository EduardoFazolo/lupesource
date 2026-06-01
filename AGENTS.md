# Lupe Agent Instructions

Run `lupe docs` for full command reference.

## Quick Start

Before starting ANY task, run:
```
lupe docs
```

This gives you the complete reference for all commands, workflows, and the merge guide.

## Core Rule

**Always use lupe to track your work.** Never manually revert files — use `lupe restore`.

Check `.lupeprivate` if it exists. If your task matches a pattern there, run `lupe private` immediately before doing anything else. Also run `lupe private` for anything involving secrets, credentials, API keys, vulnerabilities, or anything the user marks sensitive.


<!-- lupe-agent-workflow -->
# Lupe Agent Workflow

Lupe is prompt-driven source control for agents.

## Privacy — MANDATORY

**Before starting ANY task, check `.lupeprivate` if it exists, then check the
user's prompt against it. If any keyword matches, or any file being touched
matches a path pattern — run `lupe private` immediately. No exceptions.**

Built-in triggers — ALWAYS mark private without needing `.lupeprivate`:
- Prompt contains: secret, password, token, api key, credential, vulnerability,
  exploit, CVE, auth, private key, certificate, .env
- Task touches: .env, .env.*, secrets/, *secret*, *credential*, *private_key*
- User says: "don't log this", "keep this private", "sensitive", "confidential"

```bash
lupe private                  # mark current checkpoint private
lupe prompt --private "..."   # create private checkpoint from the start
```

## Project Setup — Do This First

At the start of any session in a new project, check for `.lupeignore`. If missing,
detect the stack and create it.

Detect stack: `package.json` → Node, `Cargo.toml` → Rust, `requirements.txt`/
`pyproject.toml` → Python, `go.mod` → Go, `pom.xml`/`build.gradle` → Java.

`.lupeignore` — what lupe does NOT snapshot. Auto-created on first `lupe prompt`
with generic defaults, but create it early with stack-specific entries
(e.g. `dist`, `.next`, `__pycache__`, `build`).

## Branches — MANDATORY

**Before modifying any file, you MUST run `lupe branch "<task-name>"` first. No exceptions.**

```bash
lupe branch "fix-login-bug"       # ALWAYS do this before touching files
lupe branches                     # list all branches
lupe restore fix-login-bug        # restore by name
```

Do NOT skip this step even for "small" changes.

Trying an alternative approach:
1. `lupe branch "<task-name>"` — FIRST, before any file changes
2. Make the change
3. `lupe save "what changed"`
4. If it works: keep going. If not: `lupe restore <branch-name>` to roll back.

## Workflow

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

**Never revert work by editing files manually. Always use `lupe restore`.**

Useful commands:

```bash
lupe history
lupe prompts
lupe branches
lupe graph
lupe search "<topic>"
lupe diff
lupe diff <checkpoint-uuid>
lupe diff <from-uuid> <to-uuid>
lupe restore <checkpoint-uuid-or-branch-name>
lupe branch "name"
lupe author
lupe author --name "Name" --email "email"
```

Lupe does not automatically see prompts unless the agent or host calls Lupe.
This file is the contract that tells agents when to call it.
<!-- /lupe-agent-workflow -->
