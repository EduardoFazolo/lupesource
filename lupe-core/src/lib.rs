use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use similar::ChangeTag;
use sqlx::{Row, SqlitePool, sqlite::SqlitePoolOptions};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path as FsPath, PathBuf},
};
use uuid::Uuid;
use walkdir::WalkDir;

// ── Public types ──────────────────────────────────────────────────────────────

pub struct WorkspaceInfo {
    pub name: String,
    pub path: PathBuf,
    pub branch: String,
}

pub struct Store {
    pub pool: SqlitePool,
    pub home: PathBuf,
    pub home_source: String,
    pub object_dir: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CheckpointView {
    pub id: Uuid,
    pub title: String,
    pub prompt: Option<String>,
    pub response: Option<String>,
    pub agent: Option<String>,
    pub session_id: Option<String>,
    pub parent_checkpoint_id: Option<Uuid>,
    pub root_hash: String,
    pub file_count: i64,
    pub created_at: DateTime<Utc>,
    pub private: bool,
    pub branch_name: String,
}

#[derive(Debug, Serialize)]
pub struct WebCheckpointData {
    pub id: Uuid,
    pub title: String,
    pub prompt: Option<String>,
    pub response: Option<String>,
    pub agent: Option<String>,
    pub session_id: Option<String>,
    pub parent_checkpoint_id: Option<Uuid>,
    pub root_hash: String,
    pub file_count: i64,
    pub created_at: DateTime<Utc>,
    #[serde(rename = "private")]
    pub is_private: bool,
    pub is_head: bool,
    pub branch_name: String,
    pub diff_added: i64,
    pub diff_modified: i64,
    pub diff_removed: i64,
}

#[derive(Debug, Serialize)]
pub struct WebGraphData {
    pub checkpoints: Vec<WebCheckpointData>,
    pub head_checkpoint_id: Option<Uuid>,
    pub project_name: String,
    pub branches: Vec<BranchView>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiffQuery {
    pub from: Option<String>,
    pub to: String,
}

#[derive(Debug, Serialize)]
pub struct WebDiffLine {
    pub kind: String, // "context" | "added" | "removed"
    pub content: String,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct WebHunk {
    pub old_start: u32,
    pub new_start: u32,
    pub lines: Vec<WebDiffLine>,
}

#[derive(Debug, Serialize)]
pub struct WebFileDiff {
    pub path: String,
    pub status: String, // "added" | "modified" | "removed"
    pub is_binary: bool,
    pub hunks: Vec<WebHunk>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub hash: String,
    pub len: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub files: Vec<FileEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiffView {
    pub from: Uuid,
    pub to: Uuid,
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub removed: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub kind: String,
    pub id: Uuid,
    pub checkpoint_id: Option<Uuid>,
    pub title: String,
    pub detail: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BranchView {
    pub name: String,
    pub head_checkpoint_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct Snapshot {
    pub root_hash: String,
    pub file_count: i64,
    pub manifest: Manifest,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AuthorConfig {
    pub name: Option<String>,
    pub email: Option<String>,
}

// ── Constants ─────────────────────────────────────────────────────────────────

pub const DOCS: &str = r#"
LUPE — agent-native source control
===================================
Run `lupe <command> --help` for full flag details on any command.

WORKFLOW
--------
Every agent session follows this pattern:

  lupe prompt "<user prompt>"      — auto-called by stop hook. Records what was asked.
  lupe save "<message>"            — snapshot workspace as a new checkpoint.
  lupe branch <name>               — create a named branch at current HEAD.
  lupe restore <checkpoint|branch> — roll back to any checkpoint. Never edit files manually to undo.

TRACKING
--------
  lupe prompt <text>            Record a user prompt + snapshot. --private to hide from history.
  lupe checkpoint <title>       Create a named checkpoint manually.
  lupe save [message]           Snapshot current workspace as a new checkpoint.
  lupe respond <text>           Attach agent response to latest checkpoint (called by hook).
  lupe title <title>            Update the title of the latest checkpoint.
  lupe private                  Flag the NEXT checkpoint as private.

HISTORY & INSPECTION
--------------------
  lupe history [--all]          List checkpoints. --all includes all branches.
  lupe graph [--all]            Visual tree of checkpoints.
  lupe prompts [--all]          List checkpoints showing only the user prompt.
  lupe diff [from] [to]         File changes between two checkpoints.
  lupe search <query>           Full-text search across prompts, titles, responses.
  lupe files <checkpoint-id>    List all files tracked in a checkpoint.
  lupe cat <file> <checkpoint>  Print file contents as they existed in a checkpoint.

  Add --show-private to history, graph, or prompts to reveal private checkpoints.

BRANCHES & WORKSPACES
---------------------
  lupe branch <name>            Create a named branch at current HEAD.
  lupe branches                 List all branches.
  lupe use <branch>             Set active branch for this directory. Writes .lupe-branch
                                so ALL subsequent checkpoints (including auto-hook ones)
                                go to that branch. Use 'main' to switch back.
  lupe restore <branch-name>    Restore workspace to a branch's head checkpoint.
  lupe workspace new <branch>   Create an isolated directory for parallel agent work.
  lupe workspace list           List active workspaces.
  lupe workspace drop <name>    Remove a workspace.

  AGENT WORKFLOW — when asked to work on a branch:
    1. lupe branch <name>          — create the branch
    2. lupe use <name>             — route your checkpoints to it (REQUIRED)
    3. ... do work, save files ...
    4. lupe use main               — switch back when done

  This ensures all auto-captured checkpoints (from the stop hook) go to the
  right branch, not main. Without `lupe use`, the hook always writes to main.

  Workspaces are isolated directories at .lupe/workspaces/<name>/.
  Files are copied from the current HEAD state. Each workspace has a .lupe-branch
  file that routes lupe checkpoints to the correct branch automatically.

  .lupeshared — list paths to SYMLINK into every workspace instead of copy.
  Use this for large shared dirs (node_modules, .venv, .env) so workspaces
  don't duplicate them. Example .lupeshared:
    node_modules
    .env

  To run a dev server from a workspace, cd into the workspace dir and run
  your dev command normally. node_modules will be available via symlink if
  .lupeshared is configured. Use a different port than main (e.g. --port 3002).

  NEVER copy workspace files into main manually. Use lupe merge workflow instead.

SETUP
-----
  lupe status / lupe init       Show current state. Init lupe in a new project.
  lupe install                  Install hooks + agent instructions into a workspace.
  lupe author --name --email    Set author identity.

MERGE WORKFLOW (manual, agent-driven)
--------------------------------------
To merge two branches:
  1. lupe graph --all                                  — identify the two branch tips
  2. lupe diff <ancestor-checkpoint> <main-tip>        — what main changed
  3. lupe diff <ancestor-checkpoint> <branch-tip>      — what branch changed
  4. lupe files <branch-checkpoint>                    — see all files in branch
  5. lupe cat <file> <branch-checkpoint>               — read a specific file from branch
  6. Resolve conflicts by writing files to disk
  7. lupe save "merged <branch-name> into main"
"#;

// ── Colors (CLI display helper) ───────────────────────────────────────────────

pub struct Colors {
    enabled: bool,
}

impl Colors {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    pub fn checkpoint(&self, value: &str) -> String {
        self.paint("35;1", value)
    }

    pub fn dim(&self, value: &str) -> String {
        self.paint("2", value)
    }

    pub fn bold(&self, value: &str) -> String {
        self.paint("1", value)
    }

    pub fn branch(&self, value: &str) -> String {
        self.paint("2;33", value)
    }

    pub fn private_cp(&self, value: &str) -> String {
        self.paint("36", value)
    }

    pub fn added(&self, value: &str) -> String {
        self.paint("32", value)
    }

    pub fn modified(&self, value: &str) -> String {
        self.paint("33", value)
    }

    pub fn removed(&self, value: &str) -> String {
        self.paint("31", value)
    }

    pub fn paint(&self, code: &str, value: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{value}\x1b[0m")
        } else {
            value.to_string()
        }
    }
}

// ── Store impl ────────────────────────────────────────────────────────────────

impl Store {
    fn head_path(&self) -> PathBuf {
        self.home.join("HEAD")
    }

    pub fn read_head(&self) -> Option<Uuid> {
        fs::read_to_string(self.head_path())
            .ok()
            .and_then(|s| Uuid::parse_str(s.trim()).ok())
    }

    pub fn write_head(&self, checkpoint_id: Uuid) -> Result<()> {
        fs::write(self.head_path(), checkpoint_id.to_string())?;
        Ok(())
    }

    pub async fn open(home: Option<PathBuf>) -> Result<Self> {
        let (home, home_source) = match home {
            Some(home) => (home, "explicit".to_string()),
            None => discover_or_start_project_home()?,
        };
        let object_dir = home.join("objects");
        fs::create_dir_all(&object_dir)
            .with_context(|| format!("failed to create {}", object_dir.display()))?;

        let db_path = home.join("lupe.db");
        let database_url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&database_url)
            .await
            .with_context(|| format!("failed to open {}", db_path.display()))?;

        migrate(&pool).await?;
        let store = Self {
            pool,
            home,
            home_source,
            object_dir,
        };
        bootstrap_head_if_missing(&store).await?;
        Ok(store)
    }

    pub async fn create_checkpoint(
        &self,
        title: String,
        prompt: Option<String>,
        agent: Option<String>,
        session_id: Option<String>,
        workspace: &FsPath,
        private: bool,
    ) -> Result<CheckpointView> {
        let private = private || self.consume_next_private();
        let checkpoint_id = Uuid::now_v7();
        let now = Utc::now();
        let snapshot = snapshot_workspace(workspace, &self.object_dir)?;

        let branch_name = read_workspace_branch(workspace);

        // Use this branch's own head as parent, falling back to global HEAD.
        let parent_checkpoint_id: Option<Uuid> = {
            let branch_head: Option<String> = sqlx::query_scalar(
                "select head_checkpoint_id from branches where name = ?1",
            )
            .bind(&branch_name)
            .fetch_optional(&self.pool)
            .await?;
            match branch_head.and_then(|s| Uuid::parse_str(&s).ok()) {
                Some(id) => Some(id),
                None => self.read_head(),
            }
        };

        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"
            insert into checkpoints
                (id, title, prompt, agent, session_id, parent_checkpoint_id, root_hash, file_count, created_at, branch_name)
            values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
        )
        .bind(checkpoint_id.to_string())
        .bind(&title)
        .bind(&prompt)
        .bind(&agent)
        .bind(&session_id)
        .bind(parent_checkpoint_id.map(|id| id.to_string()))
        .bind(&snapshot.root_hash)
        .bind(snapshot.file_count)
        .bind(now.to_rfc3339())
        .bind(&branch_name)
        .execute(&mut *tx)
        .await?;

        for file in &snapshot.manifest.files {
            sqlx::query(
                "insert into checkpoint_files (checkpoint_id, path, hash, size) values (?1, ?2, ?3, ?4)",
            )
            .bind(checkpoint_id.to_string())
            .bind(&file.path)
            .bind(&file.hash)
            .bind(file.len as i64)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        self.write_head(checkpoint_id)?;
        // Signal file watched by lupe-server SSE — fires for any branch, not just main.
        let _ = fs::write(self.home.join("tick"), checkpoint_id.to_string());

        // Advance this branch's head pointer (upsert so new branches auto-register).
        let now_str = now.to_rfc3339();
        sqlx::query(
            r#"insert into branches (name, head_checkpoint_id, created_at, updated_at)
               values (?1, ?2, ?3, ?3)
               on conflict(name) do update set head_checkpoint_id = excluded.head_checkpoint_id,
                                               updated_at = excluded.updated_at"#,
        )
        .bind(&branch_name)
        .bind(checkpoint_id.to_string())
        .bind(&now_str)
        .execute(&self.pool)
        .await?;

        if private {
            self.mark_private(checkpoint_id).await?;
        }

        Ok(CheckpointView {
            id: checkpoint_id,
            title,
            prompt,
            response: None,
            agent,
            session_id,
            parent_checkpoint_id,
            root_hash: snapshot.root_hash,
            file_count: snapshot.file_count,
            created_at: now,
            private,
            branch_name,
        })
    }

    pub async fn main_chain_checkpoint_ids(&self) -> Result<Vec<Uuid>> {
        let ids: Vec<String> = sqlx::query_scalar(
            "select id from checkpoints where branch_name = 'main' order by created_at desc",
        )
        .fetch_all(&self.pool)
        .await?;
        ids.into_iter().map(parse_uuid).collect()
    }

    pub async fn list_checkpoints(&self, all: bool, include_private: bool) -> Result<Vec<CheckpointView>> {
        let sql = match (all, include_private) {
            (true,  true)  => "select id, title, prompt, response, agent, session_id, parent_checkpoint_id, root_hash, file_count, created_at, private, branch_name from checkpoints order by created_at desc",
            (true,  false) => "select id, title, prompt, response, agent, session_id, parent_checkpoint_id, root_hash, file_count, created_at, private, branch_name from checkpoints where private = 0 order by created_at desc",
            (false, true)  => "select id, title, prompt, response, agent, session_id, parent_checkpoint_id, root_hash, file_count, created_at, private, branch_name from checkpoints where branch_name = 'main' order by created_at desc",
            (false, false) => "select id, title, prompt, response, agent, session_id, parent_checkpoint_id, root_hash, file_count, created_at, private, branch_name from checkpoints where branch_name = 'main' and private = 0 order by created_at desc",
        };
        let rows = sqlx::query(sql).fetch_all(&self.pool).await?;
        rows.into_iter().map(checkpoint_from_row).collect()
    }

    fn next_private_path(&self) -> PathBuf {
        self.home.join("next_private")
    }

    pub fn set_next_private(&self) -> Result<()> {
        fs::write(self.next_private_path(), "")?;
        Ok(())
    }

    pub fn consume_next_private(&self) -> bool {
        let path = self.next_private_path();
        if path.exists() {
            let _ = fs::remove_file(&path);
            true
        } else {
            false
        }
    }

    pub async fn mark_private(&self, checkpoint_id: Uuid) -> Result<()> {
        sqlx::query("update checkpoints set private = 1 where id = ?1")
            .bind(checkpoint_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_title(&self, title: String) -> Result<Uuid> {
        let checkpoint_id = self.latest_checkpoint_id().await?;
        sqlx::query("update checkpoints set title = ?1 where id = ?2")
            .bind(&title)
            .bind(checkpoint_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(checkpoint_id)
    }

    pub async fn set_response(&self, response: String) -> Result<Uuid> {
        let checkpoint_id = self.latest_checkpoint_id().await?;
        self.set_response_for(checkpoint_id, response.clone()).await?;
        if let Some(title) = title_from_response(&response) {
            sqlx::query("update checkpoints set title = ?1 where id = ?2")
                .bind(&title)
                .bind(checkpoint_id.to_string())
                .execute(&self.pool)
                .await?;
        }
        Ok(checkpoint_id)
    }

    pub async fn set_response_for(&self, checkpoint_id: Uuid, response: String) -> Result<()> {
        sqlx::query("update checkpoints set response = ?1 where id = ?2")
            .bind(&response)
            .bind(checkpoint_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn resolve_diff_range(
        &self,
        from: Option<Uuid>,
        to: Option<Uuid>,
    ) -> Result<(Uuid, Uuid)> {
        match (from, to) {
            (Some(from), Some(to)) => Ok((from, to)),
            (None, None) => self.latest_two_checkpoints().await,
            (Some(to), None) => {
                let parent: Option<String> = sqlx::query_scalar(
                    "select parent_checkpoint_id from checkpoints where id = ?1",
                )
                .bind(to.to_string())
                .fetch_optional(&self.pool)
                .await?;
                let from = parent
                    .ok_or_else(|| anyhow!("checkpoint {to} has no parent — nothing to compare against"))?;
                Ok((parse_uuid(from)?, to))
            }
            (None, Some(_)) => bail!("provide a single checkpoint uuid, two uuids, or no arguments"),
        }
    }

    async fn latest_two_checkpoints(&self) -> Result<(Uuid, Uuid)> {
        let head = self.read_head()
            .ok_or_else(|| anyhow!("no HEAD — run lupe prompt first"))?;

        let parent: Option<String> = sqlx::query_scalar(
            "select parent_checkpoint_id from checkpoints where id = ?1",
        )
        .bind(head.to_string())
        .fetch_optional(&self.pool)
        .await?;

        let from = parent
            .ok_or_else(|| anyhow!("only one checkpoint exists; nothing to compare against"))?;
        Ok((parse_uuid(from)?, head))
    }

    pub async fn restore_checkpoint(&self, id: Uuid, workspace: &FsPath) -> Result<CheckpointView> {
        let cp = self.get_checkpoint(id).await?;
        let manifest = self.get_manifest(id).await?;
        restore_manifest(&manifest, &self.object_dir, workspace)?;
        self.write_head(id)?;
        Ok(cp)
    }

    pub async fn get_checkpoint(&self, id: Uuid) -> Result<CheckpointView> {
        let row = sqlx::query(
            "select id, title, prompt, response, agent, session_id, parent_checkpoint_id, root_hash, file_count, created_at, private, branch_name from checkpoints where id = ?1",
        )
        .bind(id.to_string())
        .fetch_one(&self.pool)
        .await?;
        checkpoint_from_row(row)
    }

    pub async fn search(&self, query: &str) -> Result<Vec<SearchResult>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let pattern = format!("%{q}%");
        let checkpoint_rows = sqlx::query(
            r#"
            select 'checkpoint' as kind, id, null as checkpoint_id, title,
                   prompt as detail, created_at
            from checkpoints
            where (title like ?1 or prompt like ?1 or response like ?1)
              and private = 0
            order by created_at desc
            limit 20
            "#,
        )
        .bind(&pattern)
        .fetch_all(&self.pool)
        .await?;

        let mut results = Vec::new();
        for row in checkpoint_rows {
            results.push(SearchResult {
                kind: row.try_get("kind")?,
                id: parse_uuid(row.try_get::<String, _>("id")?)?,
                checkpoint_id: optional_uuid(row.try_get("checkpoint_id")?)?,
                title: row.try_get("title")?,
                detail: row.try_get("detail")?,
                created_at: parse_time(row.try_get::<String, _>("created_at")?)?,
            });
        }
        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(results)
    }

    pub async fn latest_checkpoint_id(&self) -> Result<Uuid> {
        if let Some(head) = self.read_head() {
            let exists: bool = sqlx::query_scalar(
                "select count(*) > 0 from checkpoints where id = ?1",
            )
            .bind(head.to_string())
            .fetch_optional(&self.pool)
            .await?
            .unwrap_or(false);
            if exists {
                return Ok(head);
            }
        }
        let id: Option<String> =
            sqlx::query_scalar("select id from checkpoints order by created_at desc limit 1")
                .fetch_optional(&self.pool)
                .await?;
        id.ok_or_else(|| anyhow!("no checkpoint exists; run `lupe checkpoint <title>` first"))
            .and_then(parse_uuid)
    }

    pub async fn create_branch(&self, name: String) -> Result<BranchView> {
        let head_checkpoint_id = self.read_head()
            .ok_or_else(|| anyhow!("no HEAD — run lupe prompt first"))?;
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        sqlx::query(
            r#"insert into branches (name, head_checkpoint_id, created_at, updated_at)
               values (?1, ?2, ?3, ?3)
               on conflict(name) do update set head_checkpoint_id = excluded.head_checkpoint_id,
                                               updated_at = excluded.updated_at"#,
        )
        .bind(&name)
        .bind(head_checkpoint_id.to_string())
        .bind(&now_str)
        .execute(&self.pool)
        .await?;
        Ok(BranchView { name, head_checkpoint_id, created_at: now, updated_at: now })
    }

    pub async fn list_branches(&self) -> Result<Vec<BranchView>> {
        let rows = sqlx::query(
            "select name, head_checkpoint_id, created_at, updated_at from branches order by updated_at desc",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut result = Vec::new();
        for row in rows {
            result.push(BranchView {
                name: row.try_get("name")?,
                head_checkpoint_id: parse_uuid(row.try_get::<String, _>("head_checkpoint_id")?)?,
                created_at: parse_time(row.try_get::<String, _>("created_at")?)?,
                updated_at: parse_time(row.try_get::<String, _>("updated_at")?)?,
            });
        }
        Ok(result)
    }

    pub async fn resolve_branch_name(&self, name: &str) -> Result<Uuid> {
        let head_checkpoint_id: Option<String> = sqlx::query_scalar(
            "select head_checkpoint_id from branches where name = ?1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        head_checkpoint_id
            .ok_or_else(|| anyhow!("no branch named '{name}'"))
            .and_then(parse_uuid)
    }

    pub async fn resolve_checkpoint_id(&self, s: &str) -> Result<Uuid> {
        if let Ok(id) = Uuid::parse_str(s) {
            return Ok(id);
        }
        let pattern = format!("{}%", s);
        let id: Option<String> = sqlx::query_scalar(
            "select id from checkpoints where id like ?1 limit 1",
        )
        .bind(&pattern)
        .fetch_optional(&self.pool)
        .await?;
        id.ok_or_else(|| anyhow!("no checkpoint matching '{s}'"))
            .and_then(parse_uuid)
    }

    pub async fn create_workspace(&self, branch_name: &str, source_workspace: &FsPath) -> Result<PathBuf> {
        let checkpoint_id = self.read_head()
            .ok_or_else(|| anyhow!("no HEAD — run lupe prompt first"))?;
        let ws_dir = self.home.join("workspaces").join(branch_name);
        if ws_dir.exists() {
            bail!("workspace '{branch_name}' already exists — drop it first with: lupe workspace drop {branch_name}");
        }

        let shared = read_lupeshared(source_workspace);
        let manifest = self.get_manifest(checkpoint_id).await?;
        fs::create_dir_all(&ws_dir)?;

        for file in &manifest.files {
            let dest = ws_dir.join(&file.path);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            let src = object_path(&self.object_dir, &file.hash)?;
            fs::copy(src, &dest)?;
        }

        for shared_path in &shared {
            let target = source_workspace.join(shared_path);
            let link = ws_dir.join(shared_path);
            if target.exists() && !link.exists() {
                if let Some(parent) = link.parent() {
                    fs::create_dir_all(parent)?;
                }
                #[cfg(unix)]
                std::os::unix::fs::symlink(&target, &link)?;
            }
        }

        for config in &[".lupeignore", ".lupeshared"] {
            let target = source_workspace.join(config);
            let link = ws_dir.join(config);
            if target.exists() && !link.exists() {
                #[cfg(unix)]
                std::os::unix::fs::symlink(&target, &link)?;
            }
        }

        fs::write(ws_dir.join(".lupe-head"), checkpoint_id.to_string())?;
        fs::write(ws_dir.join(".lupe-branch"), branch_name)?;

        // Register the branch pointing to the starting checkpoint.
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"insert into branches (name, head_checkpoint_id, created_at, updated_at)
               values (?1, ?2, ?3, ?3)
               on conflict(name) do update set head_checkpoint_id = excluded.head_checkpoint_id,
                                               updated_at = excluded.updated_at"#,
        )
        .bind(branch_name)
        .bind(checkpoint_id.to_string())
        .bind(&now)
        .execute(&self.pool)
        .await?;

        Ok(ws_dir)
    }

    pub fn list_workspaces(&self) -> Result<Vec<WorkspaceInfo>> {
        let root = self.home.join("workspaces");
        if !root.exists() {
            return Ok(Vec::new());
        }
        let mut workspaces = Vec::new();
        for entry in fs::read_dir(&root)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                let path = entry.path();
                let branch = fs::read_to_string(path.join(".lupe-branch"))
                    .or_else(|_| fs::read_to_string(path.join(".lupe-fork")))
                    .unwrap_or_else(|_| name.clone())
                    .trim()
                    .to_string();
                workspaces.push(WorkspaceInfo { name, path, branch });
            }
        }
        workspaces.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(workspaces)
    }

    pub fn drop_workspace(&self, name: &str) -> Result<()> {
        let ws_dir = self.home.join("workspaces").join(name);
        if !ws_dir.exists() {
            bail!("workspace '{name}' not found");
        }
        fs::remove_dir_all(&ws_dir)?;
        Ok(())
    }

    fn author_path(&self) -> PathBuf {
        self.home.join("author.json")
    }

    pub fn read_author(&self) -> AuthorConfig {
        fs::read_to_string(self.author_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn write_author(&self, author: &AuthorConfig) -> Result<()> {
        fs::write(self.author_path(), serde_json::to_string_pretty(author)?)?;
        Ok(())
    }

    pub async fn get_manifest(&self, checkpoint_id: Uuid) -> Result<Manifest> {
        let rows: Vec<(String, String, i64)> = sqlx::query_as(
            "select path, hash, size from checkpoint_files where checkpoint_id = ?1 order by path",
        )
        .bind(checkpoint_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        if !rows.is_empty() {
            return Ok(Manifest {
                files: rows
                    .into_iter()
                    .map(|(path, hash, len)| FileEntry {
                        path,
                        hash,
                        len: len as u64,
                    })
                    .collect(),
            });
        }

        // Legacy: try save_files via the seq=0 save for this checkpoint
        let save_id: Option<String> = sqlx::query_scalar(
            "select id from saves where checkpoint_id = ?1 order by sequence asc limit 1",
        )
        .bind(checkpoint_id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        if let Some(sid) = save_id {
            let save_rows: Vec<(String, String, i64)> = sqlx::query_as(
                "select path, hash, size from save_files where save_id = ?1 order by path",
            )
            .bind(&sid)
            .fetch_all(&self.pool)
            .await?;
            if !save_rows.is_empty() {
                return Ok(Manifest {
                    files: save_rows
                        .into_iter()
                        .map(|(path, hash, len)| FileEntry { path, hash, len: len as u64 })
                        .collect(),
                });
            }
            // Legacy: manifest JSON blob
            let value: Option<String> = sqlx::query_scalar("select manifest from saves where id = ?1")
                .bind(&sid)
                .fetch_optional(&self.pool)
                .await?;
            if let Some(v) = value {
                return Ok(serde_json::from_str(&v)?);
            }
        }

        Ok(Manifest { files: vec![] })
    }

    pub async fn diff_checkpoints(&self, from: Uuid, to: Uuid) -> Result<DiffView> {
        let added: Vec<String> = sqlx::query_scalar(
            "select path from checkpoint_files where checkpoint_id = ?1
             and path not in (select path from checkpoint_files where checkpoint_id = ?2)
             order by path",
        )
        .bind(to.to_string())
        .bind(from.to_string())
        .fetch_all(&self.pool)
        .await?;

        let removed: Vec<String> = sqlx::query_scalar(
            "select path from checkpoint_files where checkpoint_id = ?1
             and path not in (select path from checkpoint_files where checkpoint_id = ?2)
             order by path",
        )
        .bind(from.to_string())
        .bind(to.to_string())
        .fetch_all(&self.pool)
        .await?;

        let modified: Vec<String> = sqlx::query_scalar(
            "select cf1.path from checkpoint_files cf1
             join checkpoint_files cf2 on cf1.path = cf2.path and cf2.checkpoint_id = ?2
             where cf1.checkpoint_id = ?1 and cf1.hash != cf2.hash
             order by cf1.path",
        )
        .bind(from.to_string())
        .bind(to.to_string())
        .fetch_all(&self.pool)
        .await?;

        Ok(DiffView {
            from,
            to,
            added,
            modified,
            removed,
        })
    }

    pub async fn build_web_graph_data(&self, all: bool, show_private: bool) -> Result<WebGraphData> {
        let checkpoints = self.list_checkpoints(all, show_private).await?;
        let head_checkpoint_id = self.read_head();

        let mut web_checkpoints: Vec<WebCheckpointData> = Vec::new();
        for cp in &checkpoints {
            let is_head_cp = head_checkpoint_id == Some(cp.id);
            let (diff_added, diff_modified, diff_removed) =
                diff_file_counts(&self.pool, cp.parent_checkpoint_id, cp.id).await?;
            web_checkpoints.push(WebCheckpointData {
                id: cp.id,
                title: cp.title.clone(),
                prompt: cp.prompt.clone(),
                response: cp.response.clone(),
                agent: cp.agent.clone(),
                session_id: cp.session_id.clone(),
                parent_checkpoint_id: cp.parent_checkpoint_id,
                root_hash: cp.root_hash.clone(),
                file_count: cp.file_count,
                created_at: cp.created_at,
                is_private: cp.private,
                is_head: is_head_cp,
                branch_name: cp.branch_name.clone(),
                diff_added,
                diff_modified,
                diff_removed,
            });
        }

        let project_name = self.home.parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "project".to_string());

        let branches = self.list_branches().await?;

        Ok(WebGraphData {
            checkpoints: web_checkpoints,
            head_checkpoint_id,
            project_name,
            branches,
        })
    }
}

// ── Free functions ────────────────────────────────────────────────────────────

pub fn detect_agent(override_val: Option<String>) -> String {
    if let Some(v) = override_val {
        return v;
    }
    if let Ok(v) = std::env::var("LUPE_AGENT") {
        return v;
    }
    let name = if std::env::var("CLAUDE_CODE_VERSION").is_ok() {
        "claude-code"
    } else if std::env::var("CURSOR_EDITOR").is_ok() {
        "cursor"
    } else {
        "unknown"
    };
    let model = std::env::var("LUPE_AGENT_MODEL").unwrap_or_else(|_| "unknown".to_string());
    format!("{name}/{model}")
}

pub async fn diff_file_counts(
    pool: &SqlitePool,
    from_id: Option<Uuid>,
    to_id: Uuid,
) -> Result<(i64, i64, i64)> {
    let to = to_id.to_string();
    let from = from_id.map(|id| id.to_string());

    let added: i64 = if let Some(ref f) = from {
        sqlx::query_scalar(
            "select count(*) from checkpoint_files where checkpoint_id=?1
             and path not in (select path from checkpoint_files where checkpoint_id=?2)",
        )
        .bind(&to).bind(f).fetch_one(pool).await?
    } else {
        sqlx::query_scalar("select count(*) from checkpoint_files where checkpoint_id=?1")
            .bind(&to).fetch_one(pool).await?
    };

    let removed: i64 = if let Some(ref f) = from {
        sqlx::query_scalar(
            "select count(*) from checkpoint_files where checkpoint_id=?1
             and path not in (select path from checkpoint_files where checkpoint_id=?2)",
        )
        .bind(f).bind(&to).fetch_one(pool).await?
    } else { 0 };

    let modified: i64 = if let Some(ref f) = from {
        sqlx::query_scalar(
            "select count(*) from checkpoint_files a
             join checkpoint_files b on a.path=b.path
             where a.checkpoint_id=?1 and b.checkpoint_id=?2 and a.hash!=b.hash",
        )
        .bind(f).bind(&to).fetch_one(pool).await?
    } else { 0 };

    Ok((added, modified, removed))
}

pub async fn compute_web_diff(
    pool: &SqlitePool,
    object_dir: &FsPath,
    from_checkpoint_id: Option<Uuid>,
    to_checkpoint_id: Uuid,
) -> Result<Vec<WebFileDiff>> {
    let to_files: Vec<(String, String)> = sqlx::query_as(
        "select path, hash from checkpoint_files where checkpoint_id = ?1 order by path",
    )
    .bind(to_checkpoint_id.to_string())
    .fetch_all(pool)
    .await?;

    let from_files: Vec<(String, String)> = if let Some(from_id) = from_checkpoint_id {
        sqlx::query_as(
            "select path, hash from checkpoint_files where checkpoint_id = ?1 order by path",
        )
        .bind(from_id.to_string())
        .fetch_all(pool)
        .await?
    } else {
        vec![]
    };

    let from_map: BTreeMap<String, String> = from_files.into_iter().collect();
    let to_map: BTreeMap<String, String> = to_files.into_iter().collect();

    let mut all_paths: BTreeSet<String> = BTreeSet::new();
    all_paths.extend(from_map.keys().cloned());
    all_paths.extend(to_map.keys().cloned());

    const MAX_FILE_BYTES: u64 = 256 * 1024;
    let mut result: Vec<WebFileDiff> = Vec::new();

    for path in &all_paths {
        let old_hash = from_map.get(path);
        let new_hash = to_map.get(path);

        let status = match (old_hash, new_hash) {
            (None, Some(_)) => "added",
            (Some(_), None) => "removed",
            (Some(a), Some(b)) if a == b => continue,
            _ => "modified",
        };

        let old_bytes = if let Some(hash) = old_hash {
            let p = object_path(object_dir, hash)?;
            let meta = fs::metadata(&p).unwrap_or_else(|_| fs::metadata(".").unwrap());
            if meta.len() > MAX_FILE_BYTES {
                result.push(WebFileDiff { path: path.clone(), status: status.to_string(), is_binary: false, hunks: vec![] });
                continue;
            }
            fs::read(&p).unwrap_or_default()
        } else {
            vec![]
        };

        let new_bytes = if let Some(hash) = new_hash {
            let p = object_path(object_dir, hash)?;
            let meta = fs::metadata(&p).unwrap_or_else(|_| fs::metadata(".").unwrap());
            if meta.len() > MAX_FILE_BYTES {
                result.push(WebFileDiff { path: path.clone(), status: status.to_string(), is_binary: false, hunks: vec![] });
                continue;
            }
            fs::read(&p).unwrap_or_default()
        } else {
            vec![]
        };

        let is_binary = old_bytes.iter().any(|&b| b == 0) || new_bytes.iter().any(|&b| b == 0);
        if is_binary {
            result.push(WebFileDiff { path: path.clone(), status: status.to_string(), is_binary: true, hunks: vec![] });
            continue;
        }

        let old_str = String::from_utf8_lossy(&old_bytes);
        let new_str = String::from_utf8_lossy(&new_bytes);

        let diff = similar::TextDiff::from_lines(old_str.as_ref(), new_str.as_ref());
        let mut hunks: Vec<WebHunk> = Vec::new();

        for group in diff.grouped_ops(3) {
            let mut lines: Vec<WebDiffLine> = Vec::new();
            let mut old_start = 0u32;
            let mut new_start = 0u32;
            let mut first = true;

            for op in &group {
                for change in diff.iter_changes(op) {
                    let old_line = change.old_index().map(|i| i as u32 + 1);
                    let new_line = change.new_index().map(|i| i as u32 + 1);
                    if first {
                        old_start = old_line.unwrap_or(new_line.unwrap_or(1));
                        new_start = new_line.unwrap_or(old_line.unwrap_or(1));
                        first = false;
                    }
                    let kind = match change.tag() {
                        ChangeTag::Equal => "context",
                        ChangeTag::Insert => "added",
                        ChangeTag::Delete => "removed",
                    };
                    let content = change.value().trim_end_matches('\n').to_string();
                    lines.push(WebDiffLine { kind: kind.to_string(), content, old_line, new_line });
                }
            }
            if !lines.is_empty() {
                hunks.push(WebHunk { old_start, new_start, lines });
            }
        }

        result.push(WebFileDiff { path: path.clone(), status: status.to_string(), is_binary, hunks });
    }

    Ok(result)
}

pub fn snapshot_workspace(workspace: &FsPath, object_dir: &FsPath) -> Result<Snapshot> {
    if !workspace.is_dir() {
        bail!("workspace is not a directory: {}", workspace.display());
    }

    let ignore = read_lupeignore(workspace);
    let mut files = Vec::new();
    for entry in WalkDir::new(workspace).follow_links(false) {
        let entry = entry?;
        let path = entry.path();
        if should_skip(workspace, path, &ignore) || !entry.file_type().is_file() {
            continue;
        }

        let rel = path
            .strip_prefix(workspace)?
            .to_string_lossy()
            .replace('\\', "/");
        let (hash, len) = store_blob(path, object_dir)?;
        files.push(FileEntry {
            path: rel,
            hash,
            len,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));

    let mut root_input = BTreeMap::new();
    for file in &files {
        root_input.insert(file.path.clone(), file.hash.clone());
    }
    let root_hash = hash_bytes(&serde_json::to_vec(&root_input)?);
    let file_count = files.len() as i64;

    Ok(Snapshot {
        root_hash,
        file_count,
        manifest: Manifest { files },
    })
}

pub fn restore_manifest(manifest: &Manifest, object_dir: &FsPath, workspace: &FsPath) -> Result<()> {
    if !workspace.is_dir() {
        bail!("workspace is not a directory: {}", workspace.display());
    }
    let ignore = read_lupeignore(workspace);
    let wanted: BTreeSet<&str> = manifest
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();

    for entry in WalkDir::new(workspace)
        .follow_links(false)
        .contents_first(true)
    {
        let entry = entry?;
        let path = entry.path();
        if should_skip(workspace, path, &ignore) {
            continue;
        }
        if entry.file_type().is_file() {
            let rel = path
                .strip_prefix(workspace)?
                .to_string_lossy()
                .replace('\\', "/");
            if !wanted.contains(rel.as_str()) {
                fs::remove_file(path)?;
            }
        }
    }

    for file in &manifest.files {
        let dest = workspace.join(&file.path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let source = object_path(object_dir, &file.hash)?;
        fs::copy(source, dest)?;
    }
    Ok(())
}

pub fn store_blob(path: &FsPath, object_dir: &FsPath) -> Result<(String, u64)> {
    let mut file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let hash = hash_bytes(&bytes);
    let object_path = object_path(object_dir, &hash)?;
    let dir = object_path
        .parent()
        .context("object path should have a parent directory")?;
    fs::create_dir_all(dir)?;
    if !object_path.exists() {
        fs::write(&object_path, bytes)?;
    }
    let len = fs::metadata(path)?.len();
    Ok((hash, len))
}

pub fn object_path(object_dir: &FsPath, hash: &str) -> Result<PathBuf> {
    if hash.len() < 3 {
        bail!("invalid object hash: {hash}");
    }
    let (prefix, rest) = hash.split_at(2);
    Ok(object_dir.join(prefix).join(rest))
}

pub fn checkpoint_from_row(row: sqlx::sqlite::SqliteRow) -> Result<CheckpointView> {
    Ok(CheckpointView {
        id: parse_uuid(row.try_get::<String, _>("id")?)?,
        title: row.try_get("title")?,
        prompt: row.try_get("prompt")?,
        response: row.try_get("response")?,
        agent: row.try_get("agent")?,
        session_id: row.try_get("session_id").unwrap_or(None),
        parent_checkpoint_id: optional_uuid(row.try_get("parent_checkpoint_id").unwrap_or(None))?,
        root_hash: row.try_get::<Option<String>, _>("root_hash").unwrap_or(None).unwrap_or_default(),
        file_count: row.try_get::<Option<i64>, _>("file_count").unwrap_or(None).unwrap_or(0),
        created_at: parse_time(row.try_get::<String, _>("created_at")?)?,
        private: row.try_get::<i64, _>("private").unwrap_or(0) != 0,
        branch_name: row.try_get::<Option<String>, _>("branch_name").unwrap_or(None).unwrap_or_else(|| "main".to_string()),
    })
}

pub fn discover_or_start_project_home() -> Result<(PathBuf, String)> {
    let cwd = std::env::current_dir()?;
    let home = std::env::var("HOME").ok().map(PathBuf::from);
    for ancestor in cwd.ancestors() {
        if home.as_deref() == Some(ancestor) {
            break;
        }
        let candidate = ancestor.join(".lupe");
        if candidate.is_dir() {
            return Ok((candidate, "project".to_string()));
        }
    }
    Ok((cwd.join(".lupe"), "project-auto-started".to_string()))
}

pub fn find_home() -> Result<PathBuf> {
    let (home, _) = discover_or_start_project_home()?;
    Ok(home)
}

pub fn read_lupeignore(workspace: &FsPath) -> Vec<String> {
    let path = workspace.join(".lupeignore");
    match fs::read_to_string(&path) {
        Ok(content) => content
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| l.to_string())
            .collect(),
        Err(_) => DEFAULT_IGNORE.iter().map(|s| s.to_string()).collect(),
    }
}

pub fn read_workspace_branch(workspace: &FsPath) -> String {
    fs::read_to_string(workspace.join(".lupe-branch"))
        .unwrap_or_else(|_| "main".to_string())
        .trim()
        .to_string()
}

pub fn read_lupeshared(workspace: &FsPath) -> Vec<String> {
    let path = workspace.join(".lupeshared");
    match fs::read_to_string(&path) {
        Ok(content) => content
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| l.to_string())
            .collect(),
        Err(_) => Vec::new(),
    }
}

pub fn write_default_lupeignore(workspace: &FsPath) -> Result<()> {
    ensure_gitignore(workspace);
    let path = workspace.join(".lupeignore");
    if !path.exists() {
        fs::write(
            &path,
            "# Lupe ignore — files and directories lupe will not snapshot\n\
             .git\n\
             .lupe\n\
             target\n\
             node_modules\n\
             .next\n\
             dist\n\
             build\n\
             out\n\
             .cache\n\
             .turbo\n\
             .vercel\n\
             __pycache__\n\
             .venv\n\
             venv\n\
             coverage\n\
             *.map\n\
             *.pyc\n",
        )?;
        ensure_gitignore(workspace);
    }
    Ok(())
}

pub fn ensure_gitignore(workspace: &FsPath) {
    if !workspace.join(".git").is_dir() {
        return;
    }
    let gitignore_path = workspace.join(".gitignore");
    let existing = fs::read_to_string(&gitignore_path).unwrap_or_default();
    let entries = [".lupe/"];
    let to_add: Vec<&str> = entries
        .iter()
        .filter(|e| !existing.lines().any(|l| l.trim() == **e))
        .copied()
        .collect();
    if to_add.is_empty() {
        return;
    }
    let mut content = existing;
    if !content.ends_with('\n') && !content.is_empty() {
        content.push('\n');
    }
    content.push_str("\n# Lupe\n");
    for entry in &to_add {
        content.push_str(entry);
        content.push('\n');
    }
    let _ = fs::write(&gitignore_path, content);
}

pub fn should_skip(workspace: &FsPath, path: &FsPath, ignore: &[String]) -> bool {
    let Ok(rel) = path.strip_prefix(workspace) else {
        return true;
    };
    rel.components().any(|component| {
        let s = component.as_os_str().to_string_lossy();
        ignore.iter().any(|pattern| s.as_ref() == pattern.as_str())
    })
}

pub fn parse_uuid(value: String) -> Result<Uuid> {
    Uuid::parse_str(&value).with_context(|| format!("invalid uuid {value}"))
}

pub fn optional_uuid(value: Option<String>) -> Result<Option<Uuid>> {
    value.map(parse_uuid).transpose()
}

pub fn parse_time(value: String) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(&value)?.with_timezone(&Utc))
}

pub fn short_id(id: Uuid) -> String {
    id.to_string()[..8].to_string()
}

pub fn absolutize(path: PathBuf) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(path.canonicalize()?)
}

pub fn title_from_prompt(prompt: &str) -> String {
    let title = one_line(prompt);
    if title.is_empty() {
        "prompt".to_string()
    } else {
        const MAX: usize = 60;
        if title.len() > MAX {
            format!("{}...", &title[..MAX])
        } else {
            title
        }
    }
}

pub fn one_line(value: &str) -> String {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    const MAX: usize = 140;
    if value.len() > MAX {
        format!("{}...", &value[..MAX])
    } else {
        value
    }
}

pub fn friendly_time(dt: DateTime<Utc>) -> String {
    let day = dt.format("%-d").to_string().parse::<u32>().unwrap_or(0);
    format!(
        "{}, {} - {} {}{} {}",
        dt.format("%I:%M %p"),
        dt.format("%a"),
        dt.format("%B"),
        day,
        ordinal(day),
        dt.format("%Y"),
    )
}

fn ordinal(day: u32) -> &'static str {
    match (day % 100, day % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    }
}

pub fn git_push(workspace: &FsPath, message: &str) -> Result<()> {
    let run = |args: &[&str]| -> Result<()> {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(workspace)
            .status()
            .with_context(|| format!("failed to run git {}", args[0]))?;
        if !status.success() {
            bail!("git {} failed (exit {})", args[0], status);
        }
        Ok(())
    };

    println!("git add -A");
    run(&["add", "-A"])?;
    println!("git commit -m {:?}", message);
    run(&["commit", "-m", message])?;
    println!("git push");
    run(&["push"])?;
    println!("pushed.");
    Ok(())
}

pub fn resolve_lupe_bin(lupe_bin: Option<PathBuf>) -> Result<PathBuf> {
    let path = match lupe_bin {
        Some(path) => path,
        None => std::env::current_exe().context("failed to resolve current lupe executable")?,
    };
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    };
    path.canonicalize()
        .with_context(|| format!("lupe binary not found: {}", path.display()))
}

pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub fn shell_quote_path(path: &FsPath) -> String {
    shell_quote(&path.to_string_lossy())
}

pub fn hook_command(lupe_bin: &FsPath, script_dest: &FsPath) -> String {
    format!(
        "LUPE_BIN={} python3 {}",
        shell_quote_path(lupe_bin),
        shell_quote_path(script_dest)
    )
}

pub fn install_hooks(lupe_bin: &FsPath) -> Result<()> {
    let home_dir = std::env::var("HOME").context("HOME not set")?;
    let hooks_dir = PathBuf::from(&home_dir).join(".lupe").join("hooks");
    fs::create_dir_all(&hooks_dir)?;

    let script_dest = hooks_dir.join("stop.py");
    let script_src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../scripts/lupe-stop-hook.py");
    fs::copy(&script_src, &script_dest)
        .with_context(|| format!("failed to copy hook script from {}", script_src.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_dest)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_dest, perms)?;
    }
    println!("hook script -> {}", script_dest.display());
    println!("lupe binary -> {}", lupe_bin.display());

    let hook_cmd = hook_command(lupe_bin, &script_dest);

    // Claude Code: ~/.claude/settings.json
    let claude_settings = PathBuf::from(&home_dir)
        .join(".claude")
        .join("settings.json");
    let claude_entry = serde_json::json!({
        "hooks": [{"type": "command", "command": hook_cmd.clone()}]
    });
    if claude_settings.exists() {
        let content = fs::read_to_string(&claude_settings)?;
        let mut val: serde_json::Value = serde_json::from_str(&content)?;
        let stop = val
            .pointer_mut("/hooks/Stop")
            .and_then(|v| v.as_array_mut());
        if let Some(arr) = stop {
            let existing = arr.iter_mut().find(|e| {
                e.pointer("/hooks/0/command")
                    .and_then(|v| v.as_str())
                    .map(|s| s.contains("lupe"))
                    .unwrap_or(false)
            });
            if let Some(existing) = existing {
                existing["hooks"][0]["command"] = serde_json::json!(hook_cmd.clone());
            } else {
                arr.push(claude_entry);
            }
        } else {
            if !val.get("hooks").map(|v| v.is_object()).unwrap_or(false) {
                val["hooks"] = serde_json::json!({});
            }
            val["hooks"]["Stop"] = serde_json::json!([claude_entry]);
        }
        fs::write(&claude_settings, serde_json::to_string_pretty(&val)?)?;
        println!("claude code  -> {}", claude_settings.display());
    } else {
        if let Some(parent) = claude_settings.parent() {
            fs::create_dir_all(parent)?;
        }
        let val = serde_json::json!({
            "hooks": {
                "Stop": [claude_entry]
            }
        });
        fs::write(&claude_settings, serde_json::to_string_pretty(&val)?)?;
        println!("claude code  -> {}", claude_settings.display());
    }

    // Codex: ~/.codex/hooks.json
    let codex_hooks = PathBuf::from(&home_dir).join(".codex").join("hooks.json");
    let codex_entry = serde_json::json!({
        "Stop": [{"command": hook_cmd.clone()}]
    });
    if codex_hooks.exists() {
        let content = fs::read_to_string(&codex_hooks)?;
        let mut val: serde_json::Value = serde_json::from_str(&content)?;
        let stop = val.pointer_mut("/Stop").and_then(|v| v.as_array_mut());
        let entry = serde_json::json!({"command": hook_cmd.clone()});
        if let Some(arr) = stop {
            let existing = arr.iter_mut().find(|e| {
                e.get("command")
                    .and_then(|v| v.as_str())
                    .map(|s| s.contains("lupe"))
                    .unwrap_or(false)
            });
            if let Some(existing) = existing {
                existing["command"] = serde_json::json!(hook_cmd.clone());
            } else {
                arr.push(entry);
            }
        } else {
            val["Stop"] = serde_json::json!([entry]);
        }
        fs::write(&codex_hooks, serde_json::to_string_pretty(&val)?)?;
    } else {
        if let Some(parent) = codex_hooks.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&codex_hooks, serde_json::to_string_pretty(&codex_entry)?)?;
    }
    println!("codex        -> {}", codex_hooks.display());

    // Cursor: ~/.cursor/hooks.json
    let cursor_hooks = PathBuf::from(&home_dir).join(".cursor").join("hooks.json");
    let cursor_entry = serde_json::json!({
        "stop": [{"command": hook_cmd.clone()}]
    });
    if cursor_hooks.exists() {
        let content = fs::read_to_string(&cursor_hooks)?;
        let mut val: serde_json::Value = serde_json::from_str(&content)?;
        let stop = val.pointer_mut("/stop").and_then(|v| v.as_array_mut());
        let entry = serde_json::json!({"command": hook_cmd.clone()});
        if let Some(arr) = stop {
            let existing = arr.iter_mut().find(|e| {
                e.get("command")
                    .and_then(|v| v.as_str())
                    .map(|s| s.contains("lupe"))
                    .unwrap_or(false)
            });
            if let Some(existing) = existing {
                existing["command"] = serde_json::json!(hook_cmd.clone());
            } else {
                arr.push(entry);
            }
        } else {
            val["stop"] = serde_json::json!([entry]);
        }
        fs::write(&cursor_hooks, serde_json::to_string_pretty(&val)?)?;
    } else {
        if let Some(parent) = cursor_hooks.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&cursor_hooks, serde_json::to_string_pretty(&cursor_entry)?)?;
    }
    println!("cursor       -> {}", cursor_hooks.display());

    Ok(())
}

pub fn install_agent_instructions(workspace: &FsPath) -> Result<PathBuf> {
    let path = workspace.join("AGENTS.md");
    let section = lupe_agent_section();

    if path.exists() {
        let current = fs::read_to_string(&path)?;
        if current.contains("<!-- lupe-agent-workflow -->") {
            return Ok(path);
        }
        let updated = format!("{current}\n\n{section}");
        fs::write(&path, updated)?;
    } else {
        fs::write(&path, section)?;
    }

    Ok(path)
}

pub fn lupe_agent_section() -> &'static str {
    r#"<!-- lupe-agent-workflow -->
# Lupe Agent Workflow

Lupe is prompt-driven source control for agents.

## Privacy — MANDATORY

**Before starting ANY task, check `.lupeprivate` if it exists, then check the
user's prompt against it. If any keyword matches, or any file being touched
matches a path pattern — run `lupe private` immediately. No exceptions.**

Built-in triggers — ALWAYS mark private without needing `.lupeprivate`:
- Prompt contains: secret, password, token, api key, credential, vulnerability,
  exploit, CVE, auth, private key, certificate, .env
- Task touches: .env, .env.*, secrets/, *secret*, *credential*, *private_key*
- User says: "don't log this", "keep this private", "sensitive", "confidential"

```bash
lupe private                  # mark current checkpoint private
lupe prompt --private "..."   # create private checkpoint from the start
```

## Project Setup — Do This First

At the start of any session in a new project, check for `.lupeignore`. If missing,
detect the stack and create it.

Detect stack: `package.json` → Node, `Cargo.toml` → Rust, `requirements.txt`/
`pyproject.toml` → Python, `go.mod` → Go, `pom.xml`/`build.gradle` → Java.

`.lupeignore` — what lupe does NOT snapshot. Auto-created on first `lupe prompt`
with generic defaults, but create it early with stack-specific entries
(e.g. `dist`, `.next`, `__pycache__`, `build`).

## Branches — MANDATORY

**Before modifying any file, you MUST create a branch and activate it. No exceptions.**

```bash
lupe branch "fix-login-bug"       # 1. create branch
lupe use fix-login-bug            # 2. REQUIRED: route your checkpoints to it
# ... do work ...
lupe use main                     # 3. switch back when done
```

`lupe use <branch>` writes `.lupe-branch` to the current directory so the
auto-capture hook routes ALL checkpoints (including ones triggered by the
stop hook) to the right branch instead of main. Without this step your
work appears on main in the graph.

Do NOT skip `lupe use` even for "small" changes.

Trying an alternative approach:
1. `lupe branch "<task-name>"` — create branch
2. `lupe use "<task-name>"` — activate it (REQUIRED)
3. Make the change
4. `lupe save "what changed"`
5. If it works: keep going. If not: `lupe restore <branch-name>` to roll back.
6. `lupe use main` — deactivate when done

## Workflow

At the start of every user request that may modify files, run:

```bash
lupe prompt "<full user prompt>"
```

During work, run:

```bash
lupe save "<short description of what changed or now works>"
```

Save after each coherent functional unit, before risky changes, after tests pass,
and before restore/destructive operations.

**Never revert work by editing files manually. Always use `lupe restore`.**

Useful commands:

```bash
lupe history
lupe prompts
lupe branches
lupe graph
lupe search "<topic>"
lupe diff
lupe diff <checkpoint-uuid>
lupe diff <from-uuid> <to-uuid>
lupe restore <checkpoint-uuid-or-branch-name>
lupe branch "name"
lupe use "name"          # activate branch (routes hook checkpoints to it)
lupe use main            # deactivate (back to main)
lupe author
lupe author --name "Name" --email "email"
```

Lupe does not automatically see prompts unless the agent or host calls Lupe.
This file is the contract that tells agents when to call it.
<!-- /lupe-agent-workflow -->
"#
}

pub fn title_from_response(response: &str) -> Option<String> {
    for line in response.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with("```")
            || line.starts_with('#')
            || line.starts_with('>')
            || line.starts_with('-')
            || line.starts_with('|')
        {
            continue;
        }
        let clean = line
            .replace("**", "")
            .replace('*', "")
            .replace('_', "")
            .replace('`', "");
        let clean = clean.trim().to_string();
        if clean.len() < 10 {
            continue;
        }
        if title_contains_sensitive(&clean) {
            continue;
        }
        const MAX: usize = 80;
        let title = if clean.len() > MAX {
            let truncated = &clean[..MAX];
            match truncated.rfind(' ') {
                Some(pos) => format!("{}...", &truncated[..pos]),
                None => format!("{}...", truncated),
            }
        } else {
            clean
        };
        return Some(title);
    }
    None
}

const SENSITIVE_KEYWORDS: &[&str] = &[
    "secret", "password", "token", "api key", "api_key", "credential",
    "vulnerability", "exploit", "cve", "private key", "private_key",
    "certificate", ".env", "auth",
];

fn title_contains_sensitive(title: &str) -> bool {
    let lower = title.to_lowercase();
    SENSITIVE_KEYWORDS.iter().any(|k| lower.contains(k))
}

pub fn print_diff_lines(diff: &DiffView, colors: &Colors, pipe: &str, indent: &str) {
    const MAX: usize = 8;
    let mut lines: Vec<String> = Vec::new();
    for f in &diff.added {
        lines.push(colors.added(&format!("+ {f}")));
    }
    for f in &diff.modified {
        lines.push(colors.modified(&format!("~ {f}")));
    }
    for f in &diff.removed {
        lines.push(colors.removed(&format!("- {f}")));
    }
    let total = lines.len();
    let shown = lines.into_iter().take(MAX);
    for line in shown {
        println!("{}{}{}", pipe, indent, line);
    }
    if total > MAX {
        println!(
            "{}{}{}",
            pipe,
            indent,
            colors.dim(&format!("... +{} more", total - MAX))
        );
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

fn hash_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

const DEFAULT_IGNORE: &[&str] = &[
    ".git",
    ".lupe",
    "target",
    "node_modules",
    ".next",
    "dist",
    "build",
    "out",
    ".cache",
    ".turbo",
    ".vercel",
    "__pycache__",
    ".venv",
    "venv",
    "coverage",
    "*.map",
    "*.pyc",
];

// ── Database migration ────────────────────────────────────────────────────────

async fn migrate(pool: &SqlitePool) -> Result<()> {
    let statements = [
        r#"
        create table if not exists checkpoints (
            id text primary key,
            title text not null,
            prompt text,
            created_at text not null
        )
        "#,
        r#"
        create table if not exists saves (
            id text primary key,
            checkpoint_id text not null references checkpoints(id) on delete cascade,
            sequence integer not null,
            message text,
            root_hash text not null,
            file_count integer not null,
            manifest text not null,
            created_at text not null,
            unique (checkpoint_id, sequence)
        )
        "#,
        "create index if not exists checkpoints_created_at_idx on checkpoints (created_at desc)",
        "create index if not exists saves_created_at_idx on saves (created_at desc)",
        "create index if not exists saves_checkpoint_sequence_idx on saves (checkpoint_id, sequence)",
        r#"
        create table if not exists save_files (
            save_id text not null references saves(id) on delete cascade,
            path text not null,
            hash text not null,
            size integer not null default 0,
            primary key (save_id, path)
        )
        "#,
        "create index if not exists save_files_path_idx on save_files (path, save_id)",
        r#"
        create table if not exists checkpoint_files (
            checkpoint_id text not null,
            path text not null,
            hash text not null,
            size integer not null default 0,
            primary key (checkpoint_id, path)
        )
        "#,
        "create index if not exists checkpoint_files_path_idx on checkpoint_files (path, checkpoint_id)",
    ];

    for statement in statements {
        sqlx::query(statement).execute(pool).await?;
    }

    let has_response: bool = sqlx::query_scalar(
        "select count(*) > 0 from pragma_table_info('checkpoints') where name = 'response'",
    )
    .fetch_one(pool)
    .await?;
    if !has_response {
        sqlx::query("alter table checkpoints add column response text")
            .execute(pool)
            .await?;
    }

    let has_agent: bool = sqlx::query_scalar(
        "select count(*) > 0 from pragma_table_info('checkpoints') where name = 'agent'",
    )
    .fetch_one(pool)
    .await?;
    if !has_agent {
        sqlx::query("alter table checkpoints add column agent text")
            .execute(pool)
            .await?;
    }

    let has_parent_save_id: bool = sqlx::query_scalar(
        "select count(*) > 0 from pragma_table_info('checkpoints') where name = 'parent_save_id'",
    )
    .fetch_one(pool)
    .await?;
    if !has_parent_save_id {
        sqlx::query("alter table checkpoints add column parent_save_id text references saves(id)")
            .execute(pool)
            .await?;
    }

    let has_private: bool = sqlx::query_scalar(
        "select count(*) > 0 from pragma_table_info('checkpoints') where name = 'private'",
    )
    .fetch_one(pool)
    .await?;
    if !has_private {
        sqlx::query("alter table checkpoints add column private integer not null default 0")
            .execute(pool)
            .await?;
    }

    let has_session_id: bool = sqlx::query_scalar(
        "select count(*) > 0 from pragma_table_info('checkpoints') where name = 'session_id'",
    )
    .fetch_one(pool)
    .await?;
    if !has_session_id {
        sqlx::query("alter table checkpoints add column session_id text")
            .execute(pool)
            .await?;
    }

    let has_root_hash: bool = sqlx::query_scalar(
        "select count(*) > 0 from pragma_table_info('checkpoints') where name = 'root_hash'",
    )
    .fetch_one(pool)
    .await?;
    if !has_root_hash {
        sqlx::query("alter table checkpoints add column root_hash text not null default ''")
            .execute(pool)
            .await?;
    }

    let has_file_count: bool = sqlx::query_scalar(
        "select count(*) > 0 from pragma_table_info('checkpoints') where name = 'file_count'",
    )
    .fetch_one(pool)
    .await?;
    if !has_file_count {
        sqlx::query("alter table checkpoints add column file_count integer not null default 0")
            .execute(pool)
            .await?;
    }

    let has_parent_checkpoint_id: bool = sqlx::query_scalar(
        "select count(*) > 0 from pragma_table_info('checkpoints') where name = 'parent_checkpoint_id'",
    )
    .fetch_one(pool)
    .await?;
    if !has_parent_checkpoint_id {
        sqlx::query("alter table checkpoints add column parent_checkpoint_id text")
            .execute(pool)
            .await?;
    }

    let unmigrated: Vec<(String, String)> = sqlx::query_as(
        "select id, manifest from saves
         where id not in (select distinct save_id from save_files)
         and manifest != ''",
    )
    .fetch_all(pool)
    .await?;

    for (save_id, manifest_json) in unmigrated {
        let manifest: Manifest = match serde_json::from_str(&manifest_json) {
            Ok(m) => m,
            Err(_) => continue,
        };
        for file in manifest.files {
            sqlx::query(
                "insert or ignore into save_files (save_id, path, hash, size) values (?1, ?2, ?3, ?4)",
            )
            .bind(&save_id)
            .bind(&file.path)
            .bind(&file.hash)
            .bind(file.len as i64)
            .execute(pool)
            .await?;
        }
    }

    let needs_migration: Vec<String> = sqlx::query_scalar(
        "select id from checkpoints where root_hash = '' or root_hash is null",
    )
    .fetch_all(pool)
    .await?;

    for checkpoint_id in &needs_migration {
        let save: Option<(String, String, i64)> = sqlx::query_as(
            "select id, root_hash, file_count from saves where checkpoint_id = ?1 order by sequence asc limit 1",
        )
        .bind(checkpoint_id)
        .fetch_optional(pool)
        .await?;

        if let Some((save_id, root_hash, file_count)) = save {
            sqlx::query(
                "update checkpoints set root_hash = ?1, file_count = ?2 where id = ?3",
            )
            .bind(&root_hash)
            .bind(file_count)
            .bind(checkpoint_id)
            .execute(pool)
            .await?;

            let files: Vec<(String, String, i64)> = sqlx::query_as(
                "select path, hash, size from save_files where save_id = ?1",
            )
            .bind(&save_id)
            .fetch_all(pool)
            .await?;

            for (path, hash, size) in files {
                sqlx::query(
                    "insert or ignore into checkpoint_files (checkpoint_id, path, hash, size) values (?1, ?2, ?3, ?4)",
                )
                .bind(checkpoint_id)
                .bind(&path)
                .bind(&hash)
                .bind(size)
                .execute(pool)
                .await?;
            }
        }
    }

    let needs_parent_migration: Vec<(String, String)> = sqlx::query_as(
        "select id, parent_save_id from checkpoints
         where parent_save_id is not null and parent_checkpoint_id is null",
    )
    .fetch_all(pool)
    .await?;

    for (checkpoint_id, parent_save_id) in &needs_parent_migration {
        let owner: Option<String> = sqlx::query_scalar(
            "select checkpoint_id from saves where id = ?1",
        )
        .bind(parent_save_id)
        .fetch_optional(pool)
        .await?;

        if let Some(owner_id) = owner {
            sqlx::query(
                "update checkpoints set parent_checkpoint_id = ?1 where id = ?2",
            )
            .bind(&owner_id)
            .bind(checkpoint_id)
            .execute(pool)
            .await?;
        }
    }

    // ── Branches ──────────────────────────────────────────────────────────────

    sqlx::query(r#"
        create table if not exists branches (
            name text primary key,
            head_checkpoint_id text not null,
            created_at text not null,
            updated_at text not null
        )
    "#)
    .execute(pool)
    .await?;

    let has_head_checkpoint_id: bool = sqlx::query_scalar(
        "select count(*) > 0 from pragma_table_info('branches') where name = 'head_checkpoint_id'",
    )
    .fetch_one(pool)
    .await?;
    if !has_head_checkpoint_id {
        sqlx::query("alter table branches add column head_checkpoint_id text not null default ''")
            .execute(pool)
            .await?;
    }

    let has_branches_created_at: bool = sqlx::query_scalar(
        "select count(*) > 0 from pragma_table_info('branches') where name = 'created_at'",
    )
    .fetch_one(pool)
    .await?;
    if !has_branches_created_at {
        sqlx::query("alter table branches add column created_at text not null default ''")
            .execute(pool)
            .await?;
    }

    let has_branches_updated_at: bool = sqlx::query_scalar(
        "select count(*) > 0 from pragma_table_info('branches') where name = 'updated_at'",
    )
    .fetch_one(pool)
    .await?;
    if !has_branches_updated_at {
        sqlx::query("alter table branches add column updated_at text not null default ''")
            .execute(pool)
            .await?;
    }

    let has_branch_name: bool = sqlx::query_scalar(
        "select count(*) > 0 from pragma_table_info('checkpoints') where name = 'branch_name'",
    )
    .fetch_one(pool)
    .await?;
    if !has_branch_name {
        sqlx::query("alter table checkpoints add column branch_name text")
            .execute(pool)
            .await?;
    }

    // One-time migration: walk HEAD backwards and mark those checkpoints as 'main'.
    let needs_branch: Vec<String> = sqlx::query_scalar(
        "select id from checkpoints where branch_name is null",
    )
    .fetch_all(pool)
    .await?;

    if !needs_branch.is_empty() {
        // All pre-existing checkpoints belong to 'main' — branches didn't exist before.
        sqlx::query("update checkpoints set branch_name = 'main' where branch_name is null")
            .execute(pool)
            .await?;
    }

    Ok(())
}

pub async fn bootstrap_head_if_missing(store: &Store) -> Result<()> {
    if store.head_path().exists() {
        let raw = fs::read_to_string(store.head_path()).unwrap_or_default();
        let raw = raw.trim();
        if let Ok(id) = Uuid::parse_str(raw) {
            let is_checkpoint: bool = sqlx::query_scalar(
                "select count(*) > 0 from checkpoints where id = ?1",
            )
            .bind(id.to_string())
            .fetch_optional(&store.pool)
            .await?
            .unwrap_or(false);

            if !is_checkpoint {
                let cp_id: Option<String> = sqlx::query_scalar(
                    "select checkpoint_id from saves where id = ?1",
                )
                .bind(id.to_string())
                .fetch_optional(&store.pool)
                .await?;

                if let Some(cp_id) = cp_id {
                    if let Ok(uuid) = Uuid::parse_str(&cp_id) {
                        store.write_head(uuid)?;
                        // Only update main branch if this checkpoint is on main.
                        let branch: Option<String> = sqlx::query_scalar(
                            "select branch_name from checkpoints where id = ?1",
                        )
                        .bind(uuid.to_string())
                        .fetch_optional(&store.pool)
                        .await?;
                        if branch.as_deref().unwrap_or("main") == "main" {
                            upsert_main_branch(&store.pool, uuid).await?;
                        }
                        return Ok(());
                    }
                }
            } else {
                // Only update main branch pointer if this HEAD checkpoint belongs to main.
                let branch: Option<String> = sqlx::query_scalar(
                    "select branch_name from checkpoints where id = ?1",
                )
                .bind(id.to_string())
                .fetch_optional(&store.pool)
                .await?;
                if branch.as_deref().unwrap_or("main") == "main" {
                    upsert_main_branch(&store.pool, id).await?;
                }
            }
        }
        return Ok(());
    }

    // No HEAD file — bootstrap from latest main checkpoint.
    let latest: Option<String> = sqlx::query_scalar(
        "select id from checkpoints where branch_name = 'main' or branch_name is null order by created_at desc limit 1",
    )
    .fetch_optional(&store.pool)
    .await?;
    if let Some(id) = latest {
        let uuid = parse_uuid(id)?;
        store.write_head(uuid)?;
        upsert_main_branch(&store.pool, uuid).await?;
    }
    Ok(())
}

async fn upsert_main_branch(pool: &SqlitePool, head_id: Uuid) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"insert into branches (name, head_checkpoint_id, created_at, updated_at)
           values ('main', ?1, ?2, ?2)
           on conflict(name) do update set head_checkpoint_id = excluded.head_checkpoint_id,
                                           updated_at = excluded.updated_at"#,
    )
    .bind(head_id.to_string())
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_handles_spaces_and_single_quotes() {
        assert_eq!(shell_quote("plain"), "'plain'");
        assert_eq!(shell_quote("two words"), "'two words'");
        assert_eq!(shell_quote("ed's lupe"), "'ed'\\''s lupe'");
    }

    #[test]
    fn hook_command_exports_lupe_bin() {
        let cmd = hook_command(
            FsPath::new("/tmp/Lupe Bin/lupe"),
            FsPath::new("/tmp/lupe hooks/stop.py"),
        );
        assert_eq!(
            cmd,
            "LUPE_BIN='/tmp/Lupe Bin/lupe' python3 '/tmp/lupe hooks/stop.py'"
        );
    }
}
