# Feature: Real-time SSE (Any Checkpoint)

Currently SSE only fires when HEAD changes — so off-branch checkpoints from parallel agents are invisible in real time.
Fix: fire on any new checkpoint insert.

Depends on: nothing (independent server change)

## Files
- `lupe-server/src/main.rs`
- `lupe-core/src/lib.rs` — `create_checkpoint` (touch signal file)

## The problem
`watch_head` polls `.lupe/HEAD` file. HEAD only advances when the main branch advances.
A parallel agent writing to `branch = "feature-x"` never touches HEAD → SSE never fires → UI doesn't update.

## Solution
Touch a `.lupe/tick` file on every `create_checkpoint` call. Server watches `.lupe/tick` mtime instead of `.lupe/HEAD`.

Alternatives considered:
- Watch `lupe.db` mtime — works but SQLite WAL writes may not update mtime reliably on all platforms
- inotify/FSEvents — overkill, platform-specific
- `.lupe/tick` touch — simple, cross-platform, reliable

## Tasks

- [ ] `create_checkpoint` in `lib.rs`: after successful commit, `fs::write(self.home.join("tick"), checkpoint_id.to_string())`. One line.

- [ ] `lupe-server/src/main.rs`: rename `watch_head` → `watch_tick`. Change watched path from `home.join("HEAD")` to `home.join("tick")`. Logic otherwise identical.

- [ ] SSE event payload: currently sends the HEAD UUID string. Change to send the new checkpoint ID (read from tick file). Client just uses it as a trigger to re-fetch `/api/graph` — payload doesn't matter much but a checkpoint ID is more useful than HEAD.

## Done when
Two parallel agents writing to different branches both trigger SSE events. UI updates in real time for all branches.
