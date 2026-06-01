# Feature: Branches Schema

Foundation. Everything else depends on this being done first.

## What
Introduce the `branches` table and `branch_name` column on `checkpoints`.
No CLI or UI changes here — pure data model.

## Files
- `lupe-core/src/lib.rs` — `migrate()` function

## Tasks

- [ ] Add `branches` table in `migrate()`:
  ```sql
  create table if not exists branches (
      name text primary key,
      head_checkpoint_id text not null,
      created_at text not null,
      updated_at text not null
  )
  ```

- [ ] Add `branch_name TEXT` column to `checkpoints` (alter table if not exists, same pattern as other migrations)

- [ ] One-time migration: for all checkpoints with `branch_name IS NULL`, walk HEAD backwards (reuse `main_chain_checkpoint_ids` logic inline) and set `branch_name = 'main'` for those on the chain. Leave truly orphaned ones as null for now.

- [ ] Bootstrap `main` branch row in `bootstrap_head_if_missing`: after resolving HEAD, ensure a `branches` row exists for `main` with `head_checkpoint_id = HEAD`.

- [ ] Add `BranchView` struct:
  ```rust
  pub struct BranchView {
      pub name: String,
      pub head_checkpoint_id: Uuid,
      pub created_at: DateTime<Utc>,
      pub updated_at: DateTime<Utc>,
  }
  ```

- [ ] Drop `forks` table migration data — forks were bookmarks, not branches. No data worth migrating. Just stop creating new rows; leave old table in place (don't drop, SQLite alter is painful — just ignore it).

## Done when
`branches` table exists, every checkpoint on main chain has `branch_name = 'main'`, `BranchView` compiles.
