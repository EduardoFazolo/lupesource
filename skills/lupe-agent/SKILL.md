# Lupe Agent Workflow

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
lupe docs                          # read this first — full reference
lupe fork "<task-name>"            # ALWAYS before touching files
lupe prompt "<full user prompt>"   # start of every task
lupe save "<description>"          # after each unit of work
lupe restore <fork-name>           # undo — never edit files manually to revert
```

## Inspection & Merge

```bash
lupe graph --all                   # full history including dead branches
lupe diff <from> <to>              # what changed between saves
lupe files <checkpoint-id>         # list files in a checkpoint
lupe cat <file> <checkpoint-id>    # read a file as it existed in a checkpoint
lupe search "<query>"              # full-text search across history
```
