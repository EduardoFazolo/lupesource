use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool, sqlite::SqlitePoolOptions};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path as FsPath, PathBuf},
};
use uuid::Uuid;
use walkdir::WalkDir;

#[derive(Parser)]
#[command(name = "lupe")]
#[command(about = "Agent-native source control", version)]
struct Cli {
    #[arg(long, env = "LUPE_HOME")]
    home: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Prompt {
        prompt: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        prev_response: Option<String>,
        #[arg(long)]
        agent: Option<String>,
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
    },
    Checkpoint {
        title: String,
        #[arg(long)]
        prompt: String,
        #[arg(long)]
        agent: Option<String>,
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
    },
    Save {
        message: Option<String>,
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
    },
    History,
    Saves {
        checkpoint: Option<Uuid>,
    },
    Prompts,
    Graph {
        #[arg(long)]
        no_color: bool,
    },
    Diff {
        from: Option<Uuid>,
        to: Option<Uuid>,
    },
    Restore {
        save: Uuid,
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
    },
    Search {
        query: String,
    },
    Respond {
        response: String,
    },
    InstallAgent {
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
    },
    Push {
        #[arg(long)]
        message: Option<String>,
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
    },
    InstallHooks,
    Status,
}

struct Store {
    pool: SqlitePool,
    home: PathBuf,
    home_source: String,
    object_dir: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct CheckpointView {
    id: Uuid,
    title: String,
    prompt: Option<String>,
    response: Option<String>,
    agent: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SaveView {
    id: Uuid,
    checkpoint_id: Uuid,
    sequence: i64,
    message: Option<String>,
    root_hash: String,
    file_count: i64,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FileEntry {
    path: String,
    hash: String,
    len: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    files: Vec<FileEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DiffView {
    from: Uuid,
    to: Uuid,
    added: Vec<String>,
    modified: Vec<String>,
    removed: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SearchResult {
    kind: String,
    id: Uuid,
    checkpoint_id: Option<Uuid>,
    title: String,
    detail: Option<String>,
    created_at: DateTime<Utc>,
}

struct Snapshot {
    root_hash: String,
    file_count: i64,
    manifest: Manifest,
}

fn detect_agent(override_val: Option<String>) -> String {
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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let store = Store::open(cli.home).await?;

    match cli.command {
        Command::Prompt {
            prompt,
            title,
            prev_response,
            agent,
            workspace,
        } => {
            if let Some(response) = prev_response {
                if let Ok(prev_id) = store.latest_checkpoint_id().await {
                    let _ = store.set_response_for(prev_id, response).await;
                }
            }
            let workspace = absolutize(workspace)?;
            let title = title.unwrap_or_else(|| title_from_prompt(&prompt));
            let agent = detect_agent(agent);
            let (checkpoint, save) = store.create_checkpoint(title, prompt, agent, &workspace).await?;
            println!(
                "prompt {} ({}) {}",
                short_id(checkpoint.id),
                checkpoint.id,
                checkpoint.title
            );
            println!(
                "save {} ({}) seq={} files={} root={}",
                short_id(save.id),
                save.id,
                save.sequence,
                save.file_count,
                &save.root_hash[..12]
            );
        }
        Command::Checkpoint {
            title,
            prompt,
            agent,
            workspace,
        } => {
            let workspace = absolutize(workspace)?;
            let agent = detect_agent(agent);
            let (checkpoint, save) = store.create_checkpoint(title, prompt, agent, &workspace).await?;
            println!(
                "checkpoint {} ({}) {}",
                short_id(checkpoint.id),
                checkpoint.id,
                checkpoint.title
            );
            println!(
                "save {} ({}) seq={} files={} root={}",
                short_id(save.id),
                save.id,
                save.sequence,
                save.file_count,
                &save.root_hash[..12]
            );
        }
        Command::Save { message, workspace } => {
            let workspace = absolutize(workspace)?;
            let save = store.create_save(None, message, &workspace).await?;
            println!(
                "save {} ({}) checkpoint={} seq={} files={} root={}",
                short_id(save.id),
                save.id,
                short_id(save.checkpoint_id),
                save.sequence,
                save.file_count,
                &save.root_hash[..12]
            );
        }
        Command::History => {
            for checkpoint in store.list_checkpoints().await? {
                println!(
                    "{} ({})  {}  {}",
                    short_id(checkpoint.id),
                    checkpoint.id,
                    checkpoint.created_at.format("%Y-%m-%d %H:%M:%S"),
                    checkpoint.title,
                );
                if let Some(agent) = &checkpoint.agent {
                    println!("  agent: {agent}");
                }
                println!("  prompt: {}", one_line(checkpoint.prompt.as_deref().unwrap_or("")));
            }
        }
        Command::Saves { checkpoint } => {
            for save in store.list_saves(checkpoint).await? {
                println!(
                    "{} ({})  c={}  seq={}  files={}  {}  {}",
                    short_id(save.id),
                    save.id,
                    short_id(save.checkpoint_id),
                    save.sequence,
                    save.file_count,
                    save.created_at.format("%Y-%m-%d %H:%M:%S"),
                    save.message.unwrap_or_default()
                );
            }
        }
        Command::Prompts => {
            for checkpoint in store.list_checkpoints().await? {
                println!(
                    "{} ({})  {}  {}",
                    short_id(checkpoint.id),
                    checkpoint.id,
                    checkpoint.created_at.format("%Y-%m-%d %H:%M:%S"),
                    checkpoint.title
                );
                if let Some(agent) = checkpoint.agent {
                    println!("agent: {agent}");
                }
                println!("prompt: {}", checkpoint.prompt.unwrap_or_default());
                if let Some(response) = checkpoint.response {
                    println!("response: {response}");
                }
                println!();
            }
        }
        Command::Graph { no_color } => {
            let colors = Colors::new(!no_color);
            let checkpoints = store.list_checkpoints().await?;
            if checkpoints.is_empty() {
                println!("no checkpoints yet");
                return Ok(());
            }

            for (index, checkpoint) in checkpoints.iter().enumerate() {
                if index > 0 {
                    println!("{}", colors.dim("│"));
                }
                println!(
                    "{} {} {} {}",
                    colors.checkpoint("◆"),
                    colors.checkpoint(&format!("checkpoint {}", short_id(checkpoint.id))),
                    colors.dim(&format!("({})", checkpoint.id)),
                    colors.bold(&checkpoint.title)
                );
                println!(
                    "{} {} {}",
                    colors.dim("│"),
                    colors.dim("prompt:"),
                    one_line(checkpoint.prompt.as_deref().unwrap_or(""))
                );
                if let Some(agent) = &checkpoint.agent {
                    println!(
                        "{} {} {}",
                        colors.dim("│"),
                        colors.dim("agent:"),
                        agent
                    );
                }

                let saves = store.list_saves(Some(checkpoint.id)).await?;
                for (save_index, save) in saves.iter().enumerate() {
                    let branch = if save_index + 1 == saves.len() {
                        "└─"
                    } else {
                        "├─"
                    };
                    let label = if save.sequence == 0 {
                        "initial"
                    } else {
                        "save"
                    };
                    println!(
                        "{} {} {} {} {} {} {}",
                        colors.dim(branch),
                        colors.save("●"),
                        colors.save(&format!("{label} {}", short_id(save.id))),
                        colors.dim(&format!("seq={}", save.sequence)),
                        colors.dim(&format!("files={}", save.file_count)),
                        colors.dim(&format!("root={}", &save.root_hash[..12])),
                        save.message.as_deref().unwrap_or("")
                    );
                }
            }
        }
        Command::Diff { from, to } => {
            let (from, to) = store.resolve_diff_range(from, to).await?;
            let diff = store.diff_saves(from, to).await?;
            println!("from {} ({from})", short_id(from));
            println!("to   {} ({to})", short_id(to));
            print_path_list("added", &diff.added);
            print_path_list("modified", &diff.modified);
            print_path_list("removed", &diff.removed);
        }
        Command::Restore { save, workspace } => {
            let workspace = absolutize(workspace)?;
            let save = store.restore_save(save, &workspace).await?;
            println!(
                "restored save {} ({}) seq={} files={}",
                short_id(save.id),
                save.id,
                save.sequence,
                save.file_count
            );
        }
        Command::Search { query } => {
            for result in store.search(&query).await? {
                let checkpoint = result
                    .checkpoint_id
                    .map(short_id)
                    .unwrap_or_else(|| "-".to_string());
                println!(
                    "{} {} ({}) c={} {} {}",
                    result.kind,
                    short_id(result.id),
                    result.id,
                    checkpoint,
                    result.created_at.format("%Y-%m-%d %H:%M:%S"),
                    result.title
                );
                if let Some(detail) = result.detail {
                    println!("  {detail}");
                }
            }
        }
        Command::Respond { response } => {
            let checkpoint_id = store.set_response(response).await?;
            println!("response saved to checkpoint {}", short_id(checkpoint_id));
        }
        Command::Push { message, workspace } => {
            let workspace = absolutize(workspace)?;
            let msg = match message {
                Some(m) => m,
                None => {
                    let checkpoint = store
                        .list_checkpoints()
                        .await?
                        .into_iter()
                        .next()
                        .ok_or_else(|| anyhow!("no checkpoint found; run `lupe prompt` first"))?;
                    checkpoint.title
                }
            };
            git_push(&workspace, &msg)?;
        }
        Command::InstallHooks => {
            install_hooks()?;
        }
        Command::InstallAgent { workspace } => {
            let workspace = absolutize(workspace)?;
            let path = install_agent_instructions(&workspace)?;
            println!("installed agent instructions {}", path.display());
        }
        Command::Status => {
            println!("lupe ok");
            println!("mode {}", store.home_source);
            println!("home {}", store.home.display());
            println!("database {}", store.home.join("lupe.db").display());
            println!("objects {}", store.object_dir.display());
        }
    }

    Ok(())
}

impl Store {
    async fn open(home: Option<PathBuf>) -> Result<Self> {
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
        Ok(Self {
            pool,
            home,
            home_source,
            object_dir,
        })
    }

    async fn create_checkpoint(
        &self,
        title: String,
        prompt: String,
        agent: String,
        workspace: &FsPath,
    ) -> Result<(CheckpointView, SaveView)> {
        let checkpoint_id = Uuid::now_v7();
        let save_id = Uuid::now_v7();
        let now = Utc::now();
        let snapshot = snapshot_workspace(workspace, &self.object_dir)?;
        let manifest = serde_json::to_string(&snapshot.manifest)?;

        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"
            insert into checkpoints (id, title, prompt, agent, created_at)
            values (?1, ?2, ?3, ?4, ?5)
            "#,
        )
        .bind(checkpoint_id.to_string())
        .bind(&title)
        .bind(&prompt)
        .bind(&agent)
        .bind(now.to_rfc3339())
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            insert into saves
                (id, checkpoint_id, sequence, message, root_hash, file_count, manifest, created_at)
            values
                (?1, ?2, 0, 'initial state', ?3, ?4, ?5, ?6)
            "#,
        )
        .bind(save_id.to_string())
        .bind(checkpoint_id.to_string())
        .bind(&snapshot.root_hash)
        .bind(snapshot.file_count)
        .bind(manifest)
        .bind(now.to_rfc3339())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        Ok((
            CheckpointView {
                id: checkpoint_id,
                title,
                prompt: Some(prompt),
                response: None,
                agent: Some(agent),
                created_at: now,
            },
            SaveView {
                id: save_id,
                checkpoint_id,
                sequence: 0,
                message: Some("initial state".to_string()),
                root_hash: snapshot.root_hash,
                file_count: snapshot.file_count,
                created_at: now,
            },
        ))
    }

    async fn create_save(
        &self,
        checkpoint_id: Option<Uuid>,
        message: Option<String>,
        workspace: &FsPath,
    ) -> Result<SaveView> {
        let checkpoint_id = match checkpoint_id {
            Some(id) => id,
            None => self.latest_checkpoint_id().await?,
        };
        let save_id = Uuid::now_v7();
        let now = Utc::now();
        let snapshot = snapshot_workspace(workspace, &self.object_dir)?;
        let manifest = serde_json::to_string(&snapshot.manifest)?;
        let next_sequence: i64 = sqlx::query_scalar(
            "select coalesce(max(sequence), -1) + 1 from saves where checkpoint_id = ?1",
        )
        .bind(checkpoint_id.to_string())
        .fetch_one(&self.pool)
        .await?;

        sqlx::query(
            r#"
            insert into saves
                (id, checkpoint_id, sequence, message, root_hash, file_count, manifest, created_at)
            values
                (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
        )
        .bind(save_id.to_string())
        .bind(checkpoint_id.to_string())
        .bind(next_sequence)
        .bind(&message)
        .bind(&snapshot.root_hash)
        .bind(snapshot.file_count)
        .bind(manifest)
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(SaveView {
            id: save_id,
            checkpoint_id,
            sequence: next_sequence,
            message,
            root_hash: snapshot.root_hash,
            file_count: snapshot.file_count,
            created_at: now,
        })
    }

    async fn list_checkpoints(&self) -> Result<Vec<CheckpointView>> {
        let rows = sqlx::query(
            "select id, title, prompt, response, agent, created_at from checkpoints order by created_at desc",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(checkpoint_from_row).collect()
    }

    async fn set_response(&self, response: String) -> Result<Uuid> {
        let checkpoint_id = self.latest_checkpoint_id().await?;
        self.set_response_for(checkpoint_id, response).await?;
        Ok(checkpoint_id)
    }

    async fn set_response_for(&self, checkpoint_id: Uuid, response: String) -> Result<()> {
        sqlx::query("update checkpoints set response = ?1 where id = ?2")
            .bind(&response)
            .bind(checkpoint_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_saves(&self, checkpoint: Option<Uuid>) -> Result<Vec<SaveView>> {
        let rows = if let Some(checkpoint) = checkpoint {
            sqlx::query(
                r#"
                select id, checkpoint_id, sequence, message, root_hash, file_count, created_at
                from saves
                where checkpoint_id = ?1
                order by sequence asc
                "#,
            )
            .bind(checkpoint.to_string())
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                select id, checkpoint_id, sequence, message, root_hash, file_count, created_at
                from saves
                order by created_at desc
                limit 100
                "#,
            )
            .fetch_all(&self.pool)
            .await?
        };
        rows.into_iter().map(save_from_row).collect()
    }

    async fn diff_saves(&self, from: Uuid, to: Uuid) -> Result<DiffView> {
        let from_manifest = self.get_manifest(from).await?;
        let to_manifest = self.get_manifest(to).await?;
        Ok(diff_manifests(from, to, &from_manifest, &to_manifest))
    }

    async fn resolve_diff_range(
        &self,
        from: Option<Uuid>,
        to: Option<Uuid>,
    ) -> Result<(Uuid, Uuid)> {
        match (from, to) {
            (Some(from), Some(to)) => Ok((from, to)),
            (None, None) => self.latest_two_saves_in_latest_checkpoint().await,
            _ => bail!("provide both FROM and TO, or no arguments for the current diff"),
        }
    }

    async fn latest_two_saves_in_latest_checkpoint(&self) -> Result<(Uuid, Uuid)> {
        let checkpoint_id = self.latest_checkpoint_id().await?;
        let rows = sqlx::query(
            r#"
            select id
            from saves
            where checkpoint_id = ?1
            order by sequence desc
            limit 2
            "#,
        )
        .bind(checkpoint_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        if rows.len() < 2 {
            bail!("current checkpoint has fewer than two saves; run `lupe save` first");
        }

        let to = parse_uuid(rows[0].try_get::<String, _>("id")?)?;
        let from = parse_uuid(rows[1].try_get::<String, _>("id")?)?;
        Ok((from, to))
    }

    async fn restore_save(&self, id: Uuid, workspace: &FsPath) -> Result<SaveView> {
        let save = self.get_save(id).await?;
        let manifest = self.get_manifest(id).await?;
        restore_manifest(&manifest, &self.object_dir, workspace)?;
        Ok(save)
    }

    async fn search(&self, query: &str) -> Result<Vec<SearchResult>> {
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
            where title like ?1 or prompt like ?1 or response like ?1
            order by created_at desc
            limit 20
            "#,
        )
        .bind(&pattern)
        .fetch_all(&self.pool)
        .await?;
        let save_rows = sqlx::query(
            r#"
            select 'save' as kind, id, checkpoint_id, coalesce(message, 'save') as title,
                   root_hash as detail, created_at
            from saves
            where message like ?1 or root_hash like ?1
            order by created_at desc
            limit 20
            "#,
        )
        .bind(&pattern)
        .fetch_all(&self.pool)
        .await?;

        let mut results = Vec::new();
        for row in checkpoint_rows.into_iter().chain(save_rows) {
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

    async fn latest_checkpoint_id(&self) -> Result<Uuid> {
        let id: Option<String> =
            sqlx::query_scalar("select id from checkpoints order by created_at desc limit 1")
                .fetch_optional(&self.pool)
                .await?;
        let id =
            id.ok_or_else(|| anyhow!("no checkpoint exists; run `lupe checkpoint <title>` first"))?;
        parse_uuid(id)
    }

    async fn get_save(&self, id: Uuid) -> Result<SaveView> {
        let row = sqlx::query(
            r#"
            select id, checkpoint_id, sequence, message, root_hash, file_count, created_at
            from saves
            where id = ?1
            "#,
        )
        .bind(id.to_string())
        .fetch_one(&self.pool)
        .await?;
        save_from_row(row)
    }

    async fn get_manifest(&self, id: Uuid) -> Result<Manifest> {
        let value: String = sqlx::query_scalar("select manifest from saves where id = ?1")
            .bind(id.to_string())
            .fetch_one(&self.pool)
            .await?;
        Ok(serde_json::from_str(&value)?)
    }
}

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
    ];

    for statement in statements {
        sqlx::query(statement).execute(pool).await?;
    }

    // Add response column if not present (ALTER TABLE has no IF NOT EXISTS in SQLite < 3.37)
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

    Ok(())
}

fn snapshot_workspace(workspace: &FsPath, object_dir: &FsPath) -> Result<Snapshot> {
    if !workspace.is_dir() {
        bail!("workspace is not a directory: {}", workspace.display());
    }

    let mut files = Vec::new();
    for entry in WalkDir::new(workspace).follow_links(false) {
        let entry = entry?;
        let path = entry.path();
        if should_skip(workspace, path) || !entry.file_type().is_file() {
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

fn diff_manifests(from: Uuid, to: Uuid, a: &Manifest, b: &Manifest) -> DiffView {
    let a_files: BTreeMap<&str, &str> = a
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.hash.as_str()))
        .collect();
    let b_files: BTreeMap<&str, &str> = b
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.hash.as_str()))
        .collect();
    let paths: BTreeSet<&str> = a_files.keys().chain(b_files.keys()).copied().collect();

    let mut added = Vec::new();
    let mut modified = Vec::new();
    let mut removed = Vec::new();
    for path in paths {
        match (a_files.get(path), b_files.get(path)) {
            (None, Some(_)) => added.push(path.to_string()),
            (Some(_), None) => removed.push(path.to_string()),
            (Some(a_hash), Some(b_hash)) if a_hash != b_hash => modified.push(path.to_string()),
            _ => {}
        }
    }

    DiffView {
        from,
        to,
        added,
        modified,
        removed,
    }
}

fn restore_manifest(manifest: &Manifest, object_dir: &FsPath, workspace: &FsPath) -> Result<()> {
    if !workspace.is_dir() {
        bail!("workspace is not a directory: {}", workspace.display());
    }
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
        if should_skip(workspace, path) {
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

fn should_skip(workspace: &FsPath, path: &FsPath) -> bool {
    let Ok(rel) = path.strip_prefix(workspace) else {
        return true;
    };
    rel.components().any(|component| {
        let s = component.as_os_str().to_string_lossy();
        matches!(s.as_ref(), ".git" | ".lupe" | "target" | "node_modules")
    })
}

fn store_blob(path: &FsPath, object_dir: &FsPath) -> Result<(String, u64)> {
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

fn object_path(object_dir: &FsPath, hash: &str) -> Result<PathBuf> {
    if hash.len() < 3 {
        bail!("invalid object hash: {hash}");
    }
    let (prefix, rest) = hash.split_at(2);
    Ok(object_dir.join(prefix).join(rest))
}

fn checkpoint_from_row(row: sqlx::sqlite::SqliteRow) -> Result<CheckpointView> {
    Ok(CheckpointView {
        id: parse_uuid(row.try_get::<String, _>("id")?)?,
        title: row.try_get("title")?,
        prompt: row.try_get("prompt")?,
        response: row.try_get("response")?,
        agent: row.try_get("agent")?,
        created_at: parse_time(row.try_get::<String, _>("created_at")?)?,
    })
}

fn save_from_row(row: sqlx::sqlite::SqliteRow) -> Result<SaveView> {
    Ok(SaveView {
        id: parse_uuid(row.try_get::<String, _>("id")?)?,
        checkpoint_id: parse_uuid(row.try_get::<String, _>("checkpoint_id")?)?,
        sequence: row.try_get("sequence")?,
        message: row.try_get("message")?,
        root_hash: row.try_get("root_hash")?,
        file_count: row.try_get("file_count")?,
        created_at: parse_time(row.try_get::<String, _>("created_at")?)?,
    })
}

fn discover_or_start_project_home() -> Result<(PathBuf, String)> {
    let cwd = std::env::current_dir()?;
    for ancestor in cwd.ancestors() {
        let candidate = ancestor.join(".lupe");
        if candidate.is_dir() {
            return Ok((candidate, "project".to_string()));
        }
    }
    Ok((cwd.join(".lupe"), "project-auto-started".to_string()))
}

fn hash_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn parse_uuid(value: String) -> Result<Uuid> {
    Uuid::parse_str(&value).with_context(|| format!("invalid uuid {value}"))
}

fn optional_uuid(value: Option<String>) -> Result<Option<Uuid>> {
    value.map(parse_uuid).transpose()
}

fn parse_time(value: String) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(&value)?.with_timezone(&Utc))
}

fn short_id(id: Uuid) -> String {
    id.to_string()[..8].to_string()
}

fn absolutize(path: PathBuf) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(path.canonicalize()?)
}

fn print_path_list(label: &str, paths: &[String]) {
    println!("{label}: {}", paths.len());
    for path in paths {
        println!("  {path}");
    }
}

struct Colors {
    enabled: bool,
}

impl Colors {
    fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    fn checkpoint(&self, value: &str) -> String {
        self.paint("35;1", value)
    }

    fn save(&self, value: &str) -> String {
        self.paint("32;1", value)
    }

    fn dim(&self, value: &str) -> String {
        self.paint("2", value)
    }

    fn bold(&self, value: &str) -> String {
        self.paint("1", value)
    }

    fn paint(&self, code: &str, value: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{value}\x1b[0m")
        } else {
            value.to_string()
        }
    }
}

fn install_hooks() -> Result<()> {
    let home_dir = std::env::var("HOME").context("HOME not set")?;
    let hooks_dir = PathBuf::from(&home_dir).join(".lupe").join("hooks");
    fs::create_dir_all(&hooks_dir)?;

    // Write hook script
    let script_dest = hooks_dir.join("stop.py");
    let script_src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/lupe-stop-hook.py");
    fs::copy(&script_src, &script_dest)
        .with_context(|| format!("failed to copy hook script from {}", script_src.display()))?;
    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_dest)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_dest, perms)?;
    }
    println!("hook script -> {}", script_dest.display());

    let hook_cmd = format!("python3 {}", script_dest.display());

    // Claude Code: ~/.claude/settings.json
    let claude_settings = PathBuf::from(&home_dir).join(".claude").join("settings.json");
    if claude_settings.exists() {
        let content = fs::read_to_string(&claude_settings)?;
        let mut val: serde_json::Value = serde_json::from_str(&content)?;
        let stop = val
            .pointer_mut("/hooks/Stop")
            .and_then(|v| v.as_array_mut());
        let entry = serde_json::json!({
            "hooks": [{"type": "command", "command": hook_cmd}]
        });
        if let Some(arr) = stop {
            let already = arr.iter().any(|e| {
                e.pointer("/hooks/0/command")
                    .and_then(|v| v.as_str())
                    .map(|s| s.contains("lupe"))
                    .unwrap_or(false)
            });
            if !already {
                arr.push(entry);
            }
        } else {
            val["hooks"]["Stop"] = serde_json::json!([entry]);
        }
        fs::write(&claude_settings, serde_json::to_string_pretty(&val)?)?;
        println!("claude code  -> {}", claude_settings.display());
    } else {
        println!("claude code  -> not found ({})", claude_settings.display());
    }

    // Codex: ~/.codex/hooks.json
    let codex_hooks = PathBuf::from(&home_dir).join(".codex").join("hooks.json");
    let codex_entry = serde_json::json!({
        "Stop": [{"command": hook_cmd}]
    });
    if codex_hooks.exists() {
        let content = fs::read_to_string(&codex_hooks)?;
        let mut val: serde_json::Value = serde_json::from_str(&content)?;
        let stop = val.pointer_mut("/Stop").and_then(|v| v.as_array_mut());
        let entry = serde_json::json!({"command": hook_cmd});
        if let Some(arr) = stop {
            let already = arr.iter().any(|e| {
                e.get("command")
                    .and_then(|v| v.as_str())
                    .map(|s| s.contains("lupe"))
                    .unwrap_or(false)
            });
            if !already {
                arr.push(entry);
            }
        } else {
            val["Stop"] = serde_json::json!([entry]);
        }
        fs::write(&codex_hooks, serde_json::to_string_pretty(&val)?)?;
    } else {
        fs::write(&codex_hooks, serde_json::to_string_pretty(&codex_entry)?)?;
    }
    println!("codex        -> {}", codex_hooks.display());

    // Cursor: ~/.cursor/hooks.json
    let cursor_hooks = PathBuf::from(&home_dir).join(".cursor").join("hooks.json");
    let cursor_entry = serde_json::json!({
        "stop": [{"command": hook_cmd}]
    });
    if cursor_hooks.exists() {
        let content = fs::read_to_string(&cursor_hooks)?;
        let mut val: serde_json::Value = serde_json::from_str(&content)?;
        let stop = val.pointer_mut("/stop").and_then(|v| v.as_array_mut());
        let entry = serde_json::json!({"command": hook_cmd});
        if let Some(arr) = stop {
            let already = arr.iter().any(|e| {
                e.get("command")
                    .and_then(|v| v.as_str())
                    .map(|s| s.contains("lupe"))
                    .unwrap_or(false)
            });
            if !already {
                arr.push(entry);
            }
        } else {
            val["stop"] = serde_json::json!([entry]);
        }
        fs::write(&cursor_hooks, serde_json::to_string_pretty(&val)?)?;
    } else {
        fs::write(&cursor_hooks, serde_json::to_string_pretty(&cursor_entry)?)?;
    }
    println!("cursor       -> {}", cursor_hooks.display());

    println!("done. restart your agents to pick up new hooks.");
    Ok(())
}

fn install_agent_instructions(workspace: &FsPath) -> Result<PathBuf> {
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

fn lupe_agent_section() -> &'static str {
    r#"<!-- lupe-agent-workflow -->
# Lupe Agent Workflow

Lupe is prompt-driven source control for agents.

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

Useful commands:

```bash
lupe history
lupe prompts
lupe saves
lupe search "<topic>"
lupe diff <from-save-uuid> <to-save-uuid>
lupe restore <save-uuid>
```

Lupe does not automatically see prompts unless the agent or host calls Lupe.
This file is the contract that tells agents when to call it.
<!-- /lupe-agent-workflow -->
"#
}

fn one_line(value: &str) -> String {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    const MAX: usize = 140;
    if value.len() > MAX {
        format!("{}...", &value[..MAX])
    } else {
        value
    }
}

fn git_push(workspace: &FsPath, message: &str) -> Result<()> {
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

fn title_from_prompt(prompt: &str) -> String {
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
