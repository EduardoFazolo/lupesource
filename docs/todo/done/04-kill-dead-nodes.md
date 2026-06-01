# Feature: Kill Dead Nodes

Remove the "dead node" / "main chain" concept entirely.
Every checkpoint belongs to a named branch — none are "dead".

Depends on: `01-branches-schema.md`

## Files
- `lupe-core/src/lib.rs` — `main_chain_checkpoint_ids`, `build_web_graph_data`, `WebCheckpointData`
- `lupe-cli/src/main.rs` — `Command::Graph` output

## Tasks

### Core

- [ ] `WebCheckpointData`: replace `is_main_chain: bool` → `branch_name: String`

- [ ] `main_chain_checkpoint_ids`: replace HEAD-walk with `SELECT id FROM checkpoints WHERE branch_name = 'main' ORDER BY created_at DESC`. Simpler and faster.

- [ ] `build_web_graph_data`: stop building `main_chain_set`. Just read `branch_name` from each checkpoint row directly. Include `branches: Vec<BranchView>` (call `list_branches()`) in `WebGraphData`.

- [ ] `WebGraphData`: add `branches: Vec<BranchView>`

- [ ] `Colors::dead()`: remove (or rename to `Colors::branch()` if still used somewhere)

### CLI graph output

- [ ] `Command::Graph` in CLI: remove "dead branch" label and the dead-children loop. Instead, after each checkpoint, print off-branch children labeled with their `branch_name`.

- [ ] `Command::History` / `Command::Prompts`: `--all` flag now means "all branches" not "include dead". Update help text.

## Done when
`lupe graph` shows branch names, no "dead" anywhere. `WebCheckpointData.branch_name` is populated. `is_main_chain` is gone from all types and logic.
