---
name: lupe-agent
description: Lupe agent-native source control workflow — checkpoints, branches, privacy. Use when working in a repository that uses LupeHub/Lupe.
---

# Lupe Agent Workflow

**DO NOT write, copy, or paraphrase these instructions into `AGENTS.md`, `CLAUDE.md`, or any project file. This skill is already installed — agents load it automatically. Lupe setup is commands only, never file edits.**

Use this skill when working in a repository that uses LupeHub/Lupe for agent-native source control.

**First thing every session: run `lupe docs` for the full command reference.**

## Privacy — MANDATORY

Before starting ANY task, check `.lupeprivate` if it exists. If any keyword matches, or any file being touched matches a path pattern — run `lupe private` immediately. No exceptions.

Built-in triggers — ALWAYS mark private without needing `.lupeprivate`:
- Prompt contains: `secret`, `password`, `token`, `api key`, `credential`, `vulnerability`, `exploit`, `CVE`, `auth`, `private key`, `certificate`, `.env`
- Task touches: `.env`, `.env.*`, `secrets/`, `*secret*`, `*credential*`, `*password*`, `*private_key*`
- User says: "don't log this", "keep this private", "sensitive", "confidential"

`lupe private` flags the NEXT checkpoint as private — call it before doing the work.

## Core Workflow

```bash
lupe docs                             # read this first — full reference
lupe prompt "<full user prompt>"      # start of every task
lupe branch "<task-name>"             # ALWAYS before touching files
lupe use "<task-name>"                # route all checkpoints here to the branch
lupe save "<description>"             # after each unit of work
lupe restore <branch-name>            # undo — never edit files manually to revert
lupe use main                         # route back to main when finished
```

## Inspection & Merge

```bash
lupe graph --all                      # full history across all branches
lupe graph --web                      # live browser graph
lupe branches                         # list all branches
lupe diff <from> <to>                 # what changed between checkpoints
lupe files <checkpoint-id>            # list files in a checkpoint
lupe cat <file> <checkpoint-id>       # read a file as it existed in a checkpoint
lupe search "<query>"                 # full-text search across history
```

## Setup

```bash
lupe init                             # setup .lupeignore and show status
lupe install                          # install stop hooks
lupe agent-install                    # install Lupe setup skill into local agents
lupe install-skill                    # install this skill into local agents
```

Lupe does not create or update `AGENTS.md`.
