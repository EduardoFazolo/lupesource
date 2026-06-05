# Feature: Branch Commands

Replace `lupe fork` / `lupe forks` with `lupe branch` / `lupe branches`.
Also fix `lupe restore` to resolve branch names.

Depends on: `01-branches-schema.md`, `02-workspace-branch-routing.md`

## Files
- `lupe-core/src/lib.rs` — remove fork fns, add branch fns
- `lupe-cli/src/main.rs` — rename commands, update restore
- `lupe-core/src/lib.rs` — `DOCS` constant

## Tasks

### Core

- [ ] Remove `create_fork`, `list_forks`, `resolve_fork_name` (and `ForkView`)

- [ ] Add `create_branch(name)` — creates a branch row at current HEAD. If branch already exists, update its `head_checkpoint_id`. Returns `BranchView`.

- [ ] Add `list_branches()` — `SELECT * FROM branches ORDER BY updated_at DESC`

- [ ] Add `resolve_branch_name(name)` — looks up `branches` table, returns `head_checkpoint_id`. Error if not found.

### CLI

- [ ] `Command::Fork { name }` → `Command::Branch { name }`. Help: "Create a named branch at the current HEAD."

- [ ] `Command::Forks` → `Command::Branches`. Help: "List all branches."

- [ ] `Command::Restore`: try UUID parse first, then `resolve_branch_name` (was `resolve_fork_name`).

- [ ] `Command::Branch` handler: call `store.create_branch(name)`, print `branch {name} -> checkpoint {short_id}`.

- [ ] `Command::Branches` handler: list branches, print `name  head={short_id}  updated={friendly_time}`.

### Docs

- [ ] `DOCS` constant in `lib.rs`: replace all `fork`/`forks`/`dead` language with `branch`/`branches`.

## Done when
`lupe branch my-feature` creates a branch. `lupe branches` lists them. `lupe restore my-feature` restores to that branch's head. Old `lupe fork` / `lupe forks` commands are gone.
