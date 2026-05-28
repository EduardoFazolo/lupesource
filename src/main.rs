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
    History {
        #[arg(long)]
        all: bool,
    },
    Saves {
        checkpoint: Option<Uuid>,
    },
    Prompts {
        #[arg(long)]
        all: bool,
    },
    Graph {
        #[arg(long)]
        no_color: bool,
        #[arg(long)]
        all: bool,
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
    Author {
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        email: Option<String>,
    },
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
    parent_save_id: Option<Uuid>,
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

#[derive(Debug, Default, Serialize, Deserialize)]
struct AuthorConfig {
    name: Option<String>,
    email: Option<String>,
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
        Command::History { all } => {
            for checkpoint in store.list_checkpoints(all).await? {
                println!(
                    "{} ({})  {}  {}",
                    short_id(checkpoint.id),
                    checkpoint.id,
                    friendly_time(checkpoint.created_at),
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
                    friendly_time(save.created_at),
                    save.message.unwrap_or_default()
                );
            }
        }
        Command::Prompts { all } => {
            for checkpoint in store.list_checkpoints(all).await? {
                println!(
                    "{} ({})  {}  {}",
                    short_id(checkpoint.id),
                    checkpoint.id,
                    friendly_time(checkpoint.created_at),
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
        Command::Graph { no_color, all } => {
            let colors = Colors::new(!no_color);
            let head_save = store.read_head();
            let main_chain_ids = store.main_chain_checkpoint_ids().await?;
            let main_chain_set: std::collections::HashSet<Uuid> =
                main_chain_ids.iter().copied().collect();

            // All checkpoints needed to render the graph
            let checkpoints = store.list_checkpoints(all).await?;
            if checkpoints.is_empty() {
                println!("no checkpoints yet");
                return Ok(());
            }

            // Build map: save_id → dead-branch checkpoints forking from it
            let all_checkpoints_for_forks = store.list_checkpoints(true).await?;
            let mut forks_from: std::collections::HashMap<Uuid, Vec<&CheckpointView>> =
                std::collections::HashMap::new();
            // We need to own the vec for lifetime reasons — collect all non-main checkpoints
            let dead_checkpoints: Vec<CheckpointView> = all_checkpoints_for_forks
                .into_iter()
                .filter(|c| !main_chain_set.contains(&c.id))
                .collect();
            for c in &dead_checkpoints {
                if let Some(psid) = c.parent_save_id {
                    forks_from.entry(psid).or_default().push(c);
                }
            }

            let head_checkpoint_id = main_chain_ids.first().copied();

            for (index, checkpoint) in checkpoints.iter().rev().enumerate() {
                if index > 0 {
                    println!("{}", colors.dim("│"));
                }
                let is_head_checkpoint = head_save.is_some()
                    && Some(checkpoint.id) == head_checkpoint_id;
                let head_marker = if is_head_checkpoint { " [HEAD]" } else { "" };
                println!(
                    "{} {} {} {} {}{}",
                    colors.checkpoint("◆"),
                    colors.checkpoint(&format!("checkpoint {}", short_id(checkpoint.id))),
                    colors.dim(&format!("({})", checkpoint.id)),
                    colors.dim(&friendly_time(checkpoint.created_at)),
                    colors.bold(&checkpoint.title),
                    head_marker,
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

                // Checkpoint overall diff: first save → last save
                if saves.len() >= 2 {
                    let first = saves.first().unwrap();
                    let last = saves.last().unwrap();
                    let overall = store.diff_saves(first.id, last.id).await?;
                    let total = overall.added.len() + overall.modified.len() + overall.removed.len();
                    if total > 0 {
                        println!(
                            "{} {} +{} ~{} -{} overall",
                            colors.dim("│"),
                            colors.dim("changes:"),
                            overall.added.len(),
                            overall.modified.len(),
                            overall.removed.len(),
                        );
                        print_diff_lines(&overall, &colors, &colors.dim("│"), "  ");
                    }
                }

                for (save_index, save) in saves.iter().enumerate() {
                    // Check if any dead branches fork from this save
                    let dead_children = forks_from.get(&save.id).map(|v| v.as_slice()).unwrap_or(&[]);
                    let has_dead = !dead_children.is_empty();
                    let is_last_save = save_index + 1 == saves.len();
                    let branch = if is_last_save && !has_dead { "└─" } else { "├─" };
                    let label = if save.sequence == 0 { "initial" } else { "save" };
                    let head_marker = if head_save == Some(save.id) { " ◄ HEAD" } else { "" };
                    println!(
                        "{} {} {} {} {} {} {}{}",
                        colors.dim(branch),
                        colors.save("●"),
                        colors.save(&format!("{label} {}", short_id(save.id))),
                        colors.dim(&format!("seq={}", save.sequence)),
                        colors.dim(&format!("files={}", save.file_count)),
                        colors.dim(&format!("root={}", &save.root_hash[..12])),
                        save.message.as_deref().unwrap_or(""),
                        head_marker,
                    );

                    // Per-save diff: show what changed vs previous save
                    if save.sequence > 0 {
                        let prev = &saves[save_index - 1];
                        let save_diff = store.diff_saves(prev.id, save.id).await?;
                        let pipe = if is_last_save && !has_dead {
                            colors.dim(" ")
                        } else {
                            colors.dim("│")
                        };
                        print_diff_lines(&save_diff, &colors, &pipe, "     ");
                    }

                    // Render dead branches forking from this save
                    for (di, dead) in dead_children.iter().enumerate() {
                        let is_last_dead = di + 1 == dead_children.len();
                        let pipe = if is_last_save && is_last_dead { " " } else { "│" };
                        println!("{}", colors.dim(&format!("{pipe}  │")));
                        println!(
                            "{}",
                            colors.dead(&format!(
                                "{pipe}  ╰─ ◆ dead branch: {} ({}) {}",
                                short_id(dead.id),
                                dead.id,
                                dead.title
                            ))
                        );
                        println!(
                            "{}",
                            colors.dead(&format!(
                                "{pipe}        prompt: {}",
                                one_line(dead.prompt.as_deref().unwrap_or(""))
                            ))
                        );
                        let dead_saves = store.list_saves(Some(dead.id)).await?;
                        for (dsi, dsave) in dead_saves.iter().enumerate() {
                            let dlabel = if dsave.sequence == 0 { "initial" } else { "save" };
                            let dbranch = if dsi + 1 == dead_saves.len() { "└─" } else { "├─" };
                            println!(
                                "{}",
                                colors.dead(&format!(
                                    "{pipe}        {dbranch} ● {dlabel} {} seq={} files={} {}",
                                    short_id(dsave.id),
                                    dsave.sequence,
                                    dsave.file_count,
                                    dsave.message.as_deref().unwrap_or("")
                                ))
                            );
                        }
                    }
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
                    friendly_time(result.created_at),
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
                        .list_checkpoints(false)
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
            let author = store.read_author();
            match (author.name.as_deref(), author.email.as_deref()) {
                (Some(n), Some(e)) => println!("author {n} <{e}>"),
                (Some(n), None) => println!("author {n} <email not set>"),
                (None, Some(e)) => println!("author <name not set> <{e}>"),
                (None, None) => println!("author not configured"),
            }
        }
        Command::Author { name, email } => {
            let mut author = store.read_author();
            let setting = name.is_some() || email.is_some();
            if let Some(n) = name {
                author.name = Some(n);
            }
            if let Some(e) = email {
                author.email = Some(e);
            }
            if setting {
                store.write_author(&author)?;
            }
            match (author.name.as_deref(), author.email.as_deref()) {
                (Some(n), Some(e)) => {
                    println!("name  {n}");
                    println!("email {e}");
                }
                (Some(n), None) => {
                    println!("name  {n}");
                    println!("email (not set)");
                }
                (None, Some(e)) => {
                    println!("name  (not set)");
                    println!("email {e}");
                }
                (None, None) => {
                    println!("author not configured");
                    println!("set with: lupe author --name \"Your Name\" --email \"your@email.com\"");
                }
            }
        }
    }

    Ok(())
}

impl Store {
    fn head_path(&self) -> PathBuf {
        self.home.join("HEAD")
    }

    pub fn read_head(&self) -> Option<Uuid> {
        fs::read_to_string(self.head_path())
            .ok()
            .and_then(|s| Uuid::parse_str(s.trim()).ok())
    }

    fn write_head(&self, save_id: Uuid) -> Result<()> {
        fs::write(self.head_path(), save_id.to_string())?;
        Ok(())
    }

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
        let store = Self {
            pool,
            home,
            home_source,
            object_dir,
        };
        bootstrap_head_if_missing(&store).await?;
        Ok(store)
    }

    async fn create_checkpoint(
        &self,
        title: String,
        prompt: String,
        agent: String,
        workspace: &FsPath,
    ) -> Result<(CheckpointView, SaveView)> {
        let parent_save_id = self.read_head();
        let checkpoint_id = Uuid::now_v7();
        let save_id = Uuid::now_v7();
        let now = Utc::now();
        let snapshot = snapshot_workspace(workspace, &self.object_dir)?;
        let manifest = serde_json::to_string(&snapshot.manifest)?;

        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"
            insert into checkpoints (id, title, prompt, agent, parent_save_id, created_at)
            values (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
        )
        .bind(checkpoint_id.to_string())
        .bind(&title)
        .bind(&prompt)
        .bind(&agent)
        .bind(parent_save_id.map(|id| id.to_string()))
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

        for file in &snapshot.manifest.files {
            sqlx::query(
                "insert into save_files (save_id, path, hash, size) values (?1, ?2, ?3, ?4)",
            )
            .bind(save_id.to_string())
            .bind(&file.path)
            .bind(&file.hash)
            .bind(file.len as i64)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        self.write_head(save_id)?;

        Ok((
            CheckpointView {
                id: checkpoint_id,
                title,
                prompt: Some(prompt),
                response: None,
                agent: Some(agent),
                parent_save_id,
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

        let mut tx = self.pool.begin().await?;
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
        .execute(&mut *tx)
        .await?;

        for file in &snapshot.manifest.files {
            sqlx::query(
                "insert into save_files (save_id, path, hash, size) values (?1, ?2, ?3, ?4)",
            )
            .bind(save_id.to_string())
            .bind(&file.path)
            .bind(&file.hash)
            .bind(file.len as i64)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        self.write_head(save_id)?;

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

    async fn main_chain_checkpoint_ids(&self) -> Result<Vec<Uuid>> {
        let mut result = Vec::new();
        let mut current_save = self.read_head();
        loop {
            let Some(save_id) = current_save else { break };
            let checkpoint_id: Option<String> = sqlx::query_scalar(
                "select checkpoint_id from saves where id = ?1",
            )
            .bind(save_id.to_string())
            .fetch_optional(&self.pool)
            .await?;
            let Some(checkpoint_id) = checkpoint_id else { break };
            let checkpoint_id = parse_uuid(checkpoint_id)?;
            if result.contains(&checkpoint_id) {
                break;
            }
            result.push(checkpoint_id);
            let parent: Option<String> = sqlx::query_scalar::<_, Option<String>>(
                "select parent_save_id from checkpoints where id = ?1",
            )
            .bind(checkpoint_id.to_string())
            .fetch_one(&self.pool)
            .await?;
            current_save = parent.and_then(|s| Uuid::parse_str(&s).ok());
        }
        Ok(result)
    }

    async fn list_checkpoints(&self, all: bool) -> Result<Vec<CheckpointView>> {
        if all {
            let rows = sqlx::query(
                "select id, title, prompt, response, agent, parent_save_id, created_at from checkpoints order by created_at desc",
            )
            .fetch_all(&self.pool)
            .await?;
            return rows.into_iter().map(checkpoint_from_row).collect();
        }

        let ids = self.main_chain_checkpoint_ids().await?;
        if ids.is_empty() {
            // No HEAD yet — fall back to all by created_at
            let rows = sqlx::query(
                "select id, title, prompt, response, agent, parent_save_id, created_at from checkpoints order by created_at desc",
            )
            .fetch_all(&self.pool)
            .await?;
            return rows.into_iter().map(checkpoint_from_row).collect();
        }

        let mut result = Vec::with_capacity(ids.len());
        for id in &ids {
            let row = sqlx::query(
                "select id, title, prompt, response, agent, parent_save_id, created_at from checkpoints where id = ?1",
            )
            .bind(id.to_string())
            .fetch_one(&self.pool)
            .await?;
            result.push(checkpoint_from_row(row)?);
        }
        Ok(result)
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

    async fn resolve_diff_range(
        &self,
        from: Option<Uuid>,
        to: Option<Uuid>,
    ) -> Result<(Uuid, Uuid)> {
        match (from, to) {
            (Some(from), Some(to)) => Ok((from, to)),
            (None, None) => self.latest_two_saves_in_latest_checkpoint().await,
            (Some(to), None) => {
                let row: Option<(String, i64)> = sqlx::query_as(
                    "select checkpoint_id, sequence from saves where id = ?1",
                )
                .bind(to.to_string())
                .fetch_optional(&self.pool)
                .await?;
                let (checkpoint_id, sequence) = row
                    .ok_or_else(|| anyhow!("save {to} not found"))?;
                if sequence == 0 {
                    bail!("save {to} is the first save in its checkpoint — nothing before it");
                }
                let from: String = sqlx::query_scalar(
                    "select id from saves where checkpoint_id = ?1 and sequence = ?2",
                )
                .bind(&checkpoint_id)
                .bind(sequence - 1)
                .fetch_one(&self.pool)
                .await?;
                Ok((parse_uuid(from)?, to))
            }
            (None, Some(_)) => bail!("provide a single save uuid, two uuids, or no arguments"),
        }
    }

    async fn latest_two_saves_in_latest_checkpoint(&self) -> Result<(Uuid, Uuid)> {
        let checkpoint_id = self.latest_checkpoint_id().await?;
        let rows = sqlx::query(
            "select id from saves where checkpoint_id = ?1 order by sequence desc limit 2",
        )
        .bind(checkpoint_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        if rows.len() >= 2 {
            let to = parse_uuid(rows[0].try_get::<String, _>("id")?)?;
            let from = parse_uuid(rows[1].try_get::<String, _>("id")?)?;
            return Ok((from, to));
        }

        // Single save in current checkpoint — compare HEAD to previous checkpoint's last save
        let head = rows
            .into_iter()
            .next()
            .map(|r| r.try_get::<String, _>("id"))
            .transpose()?
            .ok_or_else(|| anyhow!("no saves found"))?;
        let to = parse_uuid(head)?;

        let prev: Option<String> = sqlx::query_scalar(
            r#"
            select s.id from saves s
            join checkpoints c on c.id = s.checkpoint_id
            where c.id != ?1
              and s.created_at < (select created_at from saves where id = ?2)
            order by s.created_at desc
            limit 1
            "#,
        )
        .bind(checkpoint_id.to_string())
        .bind(to.to_string())
        .fetch_optional(&self.pool)
        .await?;

        let from = prev
            .ok_or_else(|| anyhow!("only one save exists; nothing to compare against"))?;
        Ok((parse_uuid(from)?, to))
    }

    async fn restore_save(&self, id: Uuid, workspace: &FsPath) -> Result<SaveView> {
        let save = self.get_save(id).await?;
        let manifest = self.get_manifest(id).await?;
        restore_manifest(&manifest, &self.object_dir, workspace)?;
        self.write_head(id)?;
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
        if let Some(head) = self.read_head() {
            let id: Option<String> = sqlx::query_scalar(
                "select checkpoint_id from saves where id = ?1",
            )
            .bind(head.to_string())
            .fetch_optional(&self.pool)
            .await?;
            if let Some(id) = id {
                return parse_uuid(id);
            }
        }
        let id: Option<String> =
            sqlx::query_scalar("select id from checkpoints order by created_at desc limit 1")
                .fetch_optional(&self.pool)
                .await?;
        id.ok_or_else(|| anyhow!("no checkpoint exists; run `lupe checkpoint <title>` first"))
            .and_then(parse_uuid)
    }

    fn author_path(&self) -> PathBuf {
        self.home.join("author.json")
    }

    fn read_author(&self) -> AuthorConfig {
        fs::read_to_string(self.author_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn write_author(&self, author: &AuthorConfig) -> Result<()> {
        fs::write(self.author_path(), serde_json::to_string_pretty(author)?)?;
        Ok(())
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
        let rows: Vec<(String, String, i64)> = sqlx::query_as(
            "select path, hash, size from save_files where save_id = ?1 order by path",
        )
        .bind(id.to_string())
        .fetch_all(&self.pool)
        .await?;

        if !rows.is_empty() {
            return Ok(Manifest {
                files: rows
                    .into_iter()
                    .map(|(path, hash, len)| FileEntry { path, hash, len: len as u64 })
                    .collect(),
            });
        }

        // Legacy: fall back to manifest JSON blob for saves before migration
        let value: String = sqlx::query_scalar("select manifest from saves where id = ?1")
            .bind(id.to_string())
            .fetch_one(&self.pool)
            .await?;
        Ok(serde_json::from_str(&value)?)
    }

    async fn diff_saves(&self, from: Uuid, to: Uuid) -> Result<DiffView> {
        let added: Vec<String> = sqlx::query_scalar(
            "select path from save_files where save_id = ?1
             and path not in (select path from save_files where save_id = ?2)
             order by path",
        )
        .bind(to.to_string())
        .bind(from.to_string())
        .fetch_all(&self.pool)
        .await?;

        let removed: Vec<String> = sqlx::query_scalar(
            "select path from save_files where save_id = ?1
             and path not in (select path from save_files where save_id = ?2)
             order by path",
        )
        .bind(from.to_string())
        .bind(to.to_string())
        .fetch_all(&self.pool)
        .await?;

        let modified: Vec<String> = sqlx::query_scalar(
            "select sf1.path from save_files sf1
             join save_files sf2 on sf1.path = sf2.path and sf2.save_id = ?2
             where sf1.save_id = ?1 and sf1.hash != sf2.hash
             order by sf1.path",
        )
        .bind(from.to_string())
        .bind(to.to_string())
        .fetch_all(&self.pool)
        .await?;

        Ok(DiffView { from, to, added, modified, removed })
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

    // Populate save_files from existing manifest blobs for saves not yet migrated
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

    Ok(())
}

async fn bootstrap_head_if_missing(store: &Store) -> Result<()> {
    if store.head_path().exists() {
        return Ok(());
    }
    let latest: Option<String> = sqlx::query_scalar(
        "select id from saves order by created_at desc limit 1",
    )
    .fetch_optional(&store.pool)
    .await?;
    if let Some(id) = latest {
        let uuid = parse_uuid(id)?;
        store.write_head(uuid)?;
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
        parent_save_id: optional_uuid(row.try_get("parent_save_id")?)?,
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

    fn dead(&self, value: &str) -> String {
        self.paint("2;33", value)
    }

    fn added(&self, value: &str) -> String {
        self.paint("32", value)
    }

    fn modified(&self, value: &str) -> String {
        self.paint("33", value)
    }

    fn removed(&self, value: &str) -> String {
        self.paint("31", value)
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

fn print_diff_lines(diff: &DiffView, colors: &Colors, pipe: &str, indent: &str) {
    const MAX: usize = 8;
    let mut lines: Vec<String> = Vec::new();
    for f in &diff.added   { lines.push(colors.added  (&format!("+ {f}"))); }
    for f in &diff.modified{ lines.push(colors.modified(&format!("~ {f}"))); }
    for f in &diff.removed { lines.push(colors.removed (&format!("- {f}"))); }
    let total = lines.len();
    let shown = lines.into_iter().take(MAX);
    for line in shown {
        println!("{}{}{}", pipe, indent, line);
    }
    if total > MAX {
        println!("{}{}{}",
            pipe, indent,
            colors.dim(&format!("... +{} more", total - MAX))
        );
    }
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

fn friendly_time(dt: DateTime<Utc>) -> String {
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
