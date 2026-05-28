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

## MVP Commands

Use the CLI directly from a workspace:

```bash
lupe status
lupe prompt "full user prompt"
lupe save "working change"
lupe history
lupe prompts
lupe saves
lupe graph
lupe search "working"
lupe diff
lupe diff <save-a-uuid> <save-b-uuid>
lupe restore <save-uuid>
lupe install-agent
```

Data model:

- `prompt`: records a full user prompt and creates the meaningful checkpoint/task node.
- `checkpoint`: lower-level command for creating a prompt-backed checkpoint manually.
- `save`: lightweight source-state snapshot inside a checkpoint.
- `object`: content-addressed file bytes stored outside SQLite.
- `graph`: colored terminal view of prompt/checkpoint nodes and nested saves.

## Agent Instructions

Agents should follow [AGENTS.md](AGENTS.md). Keep it updated whenever Lupe's CLI,
storage, terminology, or workflow changes.

There is also a portable skill draft at [skills/lupe-agent/SKILL.md](skills/lupe-agent/SKILL.md).

To add Lupe instructions to another workspace:

```bash
lupe install-agent
```
