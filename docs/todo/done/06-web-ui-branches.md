# Feature: Web UI — Branches

Update the frontend to show branches as first-class lanes.
No more "dead" label, no opacity tricks, no shame.

Depends on: `04-kill-dead-nodes.md` (needs `branch_name` in API response), `03-branch-commands.md` (needs `branches` list in GraphData)

## Files
- `web/src/types.ts`
- `web/src/Timeline.tsx`
- `web/src/App.tsx`

## Tasks

### types.ts

- [ ] `CheckpointData`: `is_main_chain: boolean` → `branch_name: string`
- [ ] Add `BranchData` interface: `{ name: string, head_checkpoint_id: string, created_at: string, updated_at: string }`
- [ ] `GraphData`: add `branches: BranchData[]`

### Timeline.tsx

- [ ] Remove `isDead` entirely — no opacity, no "dead" label, no special sizing, no `C.deadText`

- [ ] `computeLanes` rewrite: assign lanes by `branch_name`.
  - `main` always gets lane 0 (leftmost spine)
  - Other branch names sorted alphabetically → stable lane index 1, 2, 3...
  - `ForkLane.color` driven by branch name hash, not session_id
  - Remove `MAX_LANES` cap or raise it — branches are real, don't hide them

- [ ] Branch label: where "dead" badge was, show the branch name instead (small colored pill using the branch color)

- [ ] Diff stats: show on all checkpoints regardless of branch (currently only shown on `!isDead`)

- [ ] Remove `C.deadText` color constant

### App.tsx

- [ ] Top bar: remove `"N dead"` counter → `"N branches"` (count distinct branch names)
- [ ] MOCK data: replace `is_main_chain: true/false` with `branch_name: 'main'` / `branch_name: 'try-postgres'` etc.

## Done when
The web UI shows all branches as colored lanes. Branch names visible on each checkpoint. No "dead" anywhere. Multiple parallel agent branches visible simultaneously in real time (once 05-realtime-sse is also done).
