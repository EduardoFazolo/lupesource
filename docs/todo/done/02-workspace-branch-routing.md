# Feature: Workspace = Branch Context

The mechanic that makes multi-agent branching work.
When an agent runs in a workspace directory, lupe auto-detects the branch from a `.lupe-branch` file — no env vars, no config.

Depends on: `01-branches-schema.md`

## How it works
- `lupe workspace new <name>` creates `.lupe/workspaces/<name>/` and writes `.lupe-branch` containing the branch name
- When `lupe prompt`/`lupe save` runs with `--workspace <path>`, `create_checkpoint` reads `.lupe-branch` from that path
- If no `.lupe-branch` found → branch is `"main"`
- After creating checkpoint, advance the branch's `head_checkpoint_id`

## Files
- `lupe-core/src/lib.rs` — `create_checkpoint`, `create_workspace`, `list_workspaces`
- `lupe-cli/src/main.rs` — `WorkspaceAction::New`, `WorkspaceAction::List`

## Tasks

- [ ] `create_checkpoint`: before inserting, read `workspace.join(".lupe-branch")` → `branch_name` string, default `"main"`. Store it in the checkpoint row. After insert, `UPDATE branches SET head_checkpoint_id = ?, updated_at = ? WHERE name = ?` (upsert if branch doesn't exist yet).

- [ ] `create_workspace`: rename param `fork_name` → `branch_name`. Write `.lupe-branch` file (instead of `.lupe-fork`). Create a `branches` row for the new branch pointing to the starting checkpoint.

- [ ] `list_workspaces`: read `.lupe-branch` file. Backwards compat: if `.lupe-branch` missing but `.lupe-fork` exists, read `.lupe-fork` as branch name (no hard migration needed on disk).

- [ ] `WorkspaceInfo`: rename field `fork: String` → `branch: String`

- [ ] CLI `WorkspaceAction::New { fork }` → `{ branch }`. Help text update.

- [ ] CLI `WorkspaceAction::List`: print `branch=` not `fork=`

## Done when
`lupe workspace new my-feature` creates a workspace with `.lupe-branch = "my-feature"`.
Any `lupe prompt` run inside that workspace creates a checkpoint with `branch_name = "my-feature"` and advances the branch head.
