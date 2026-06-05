---
name: lupe-agent-install
description: Install and set up Lupe in a project — build, hooks, skills, init. Use when a user asks to install Lupe, set up Lupe, or enable Lupe for agents.
---

# Lupe Install And Start

**DO NOT write, copy, or paraphrase any Lupe instructions into `AGENTS.md`, `CLAUDE.md`, or any project file. "Enable Lupe for agents" means run the install commands below — the skills install themselves and agents load them automatically. Setup is commands only, never file edits.**

Use this skill when a user asks to install Lupe, set up Lupe in a project, enable Lupe for agents, or start using Lupe for agent-native source control.

## Install Checklist

1. Check whether `lupe` is available:

```bash
command -v lupe
lupe docs
```

2. If `lupe` is not installed but this repository is available, build or install it from the repository root:

```bash
cargo build --release
cargo install --path lupe-cli
```

3. Install agent stop hooks:

```bash
lupe install
```

4. Install Lupe skills for agents:

```bash
lupe agent-install
lupe install-skill
```

Use `--agent codex`, `--agent claude`, or `--agent cursor` to target one agent.

## Start Using Lupe In A Project

Run these commands from the project root:

```bash
lupe init
lupe docs
lupe prompt "<full user prompt>"
lupe branch "<task-name>"
lupe use "<task-name>"
```

During work:

```bash
lupe save "<what changed>"
lupe diff
lupe graph --all
```

When finished:

```bash
lupe save "<final result>"
lupe use main
```

## Rules

- Never create or edit `AGENTS.md` as part of Lupe setup.
- Use `lupe restore <checkpoint-or-branch>` to undo Lupe-tracked work.
- For sensitive work, run `lupe private` before the checkpoint.
