use anyhow::{Result, anyhow, bail};
use clap::{Parser, Subcommand};
use lupe_core::{
    Colors, Store,
    absolutize, detect_agent,
    friendly_time, git_push, install_agent_instructions, install_hooks,
    one_line, print_diff_lines, resolve_lupe_bin, short_id,
    title_from_prompt, write_default_lupeignore, DOCS,
};
use std::path::PathBuf;
use uuid::Uuid;

// ── CLI types ─────────────────────────────────────────────────────────────────

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
    #[command(about = "Record a user prompt and snapshot the workspace. Called by the stop hook automatically.")]
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
        #[arg(long)]
        session: Option<String>,
        #[arg(long, help = "Mark this checkpoint private (hidden from history/graph by default)")]
        private: bool,
    },
    #[command(about = "Create a named checkpoint manually with a title and prompt.")]
    Checkpoint {
        title: String,
        #[arg(long)]
        prompt: String,
        #[arg(long)]
        agent: Option<String>,
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
    },
    #[command(about = "Snapshot the current workspace state as a new checkpoint. Use after completing a meaningful unit of work.")]
    Save {
        message: Option<String>,
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
    },
    #[command(about = "List checkpoints. Defaults to main branch. Use --all to include all branches.")]
    History {
        #[arg(long)]
        all: bool,
        #[arg(long, help = "Include private checkpoints (hidden by default)")]
        show_private: bool,
    },
    #[command(about = "List checkpoints showing only the user prompt. Defaults to main branch. Use --all for all branches.")]
    Prompts {
        #[arg(long)]
        all: bool,
        #[arg(long)]
        show_private: bool,
    },
    #[command(about = "Visual tree of checkpoints. Defaults to main branch. Use --all to show all branches.")]
    Graph {
        #[arg(long)]
        no_color: bool,
        #[arg(long)]
        all: bool,
        #[arg(long, help = "Reveal private checkpoint titles and prompts")]
        show_private: bool,
        #[arg(long, help = "Open interactive graph in browser")]
        web: bool,
        #[arg(long, default_value = "4747", help = "Port for --web server")]
        port: u16,
    },
    #[command(about = "Show file changes between two checkpoints. Defaults to last two checkpoints if omitted.")]
    Diff {
        from: Option<Uuid>,
        to: Option<Uuid>,
    },
    #[command(about = "Restore workspace files to a specific checkpoint state. Also accepts a branch name.")]
    Restore {
        checkpoint: String,
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
    },
    #[command(about = "Full-text search across checkpoint titles, prompts, and responses.")]
    Search {
        query: String,
    },
    #[command(about = "Attach an agent response to the latest checkpoint. Called by the stop hook automatically.")]
    Respond {
        response: String,
    },
    #[command(about = "Install lupe hooks and agent instructions into a workspace.")]
    Install {
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
        #[arg(long)]
        lupe_bin: Option<PathBuf>,
        #[arg(long)]
        no_agent: bool,
        #[arg(long)]
        no_hooks: bool,
    },
    #[command(about = "Install agent instructions (AGENTS.md) into a workspace.")]
    InstallAgent {
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
    },
    #[command(about = "Push workspace changes to git remote with a lupe checkpoint message.")]
    Push {
        #[arg(long)]
        message: Option<String>,
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
    },
    #[command(about = "Install the lupe stop hook into the global Claude Code hooks config.")]
    InstallHooks {
        #[arg(long)]
        lupe_bin: Option<PathBuf>,
    },
    #[command(about = "Show current lupe status: HEAD checkpoint, workspace, and recent activity.")]
    Status,
    #[command(about = "Alias for status. Initializes lupe in the current project if not already set up.")]
    Init,
    #[command(about = "Set or update the title of the latest checkpoint.")]
    Title {
        title: String,
    },
    #[command(about = "Create a named branch at the current HEAD checkpoint. Use before risky or parallel work.")]
    Branch {
        name: String,
    },
    #[command(about = "List all branches and their head checkpoints.")]
    Branches,
    #[command(about = "Manage isolated workspaces for parallel agent work.")]
    Workspace {
        #[command(subcommand)]
        action: WorkspaceAction,
    },
    #[command(about = "Print the contents of a file as it existed in a checkpoint.")]
    Cat {
        file: String,
        checkpoint: String,
    },
    #[command(about = "List all files tracked in a checkpoint. Useful before a merge to see what each fork contains.")]
    Files {
        checkpoint: String,
    },
    #[command(about = "Flag the next checkpoint as private. Private checkpoints are hidden from history and graph by default.")]
    Private,
    #[command(about = "Show or set the author name and email recorded on checkpoints.")]
    Author {
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        email: Option<String>,
    },
    #[command(about = "Print full reference documentation for all lupe commands. Agents should run this to understand available tools.")]
    Docs,
    #[command(about = "Set the active branch for this working directory. Writes .lupe-branch so all subsequent checkpoints (including hook-triggered ones) go to that branch. Use 'main' to switch back.")]
    Use {
        branch: String,
    },
}

#[derive(Subcommand)]
enum WorkspaceAction {
    New {
        branch: String,
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
    },
    List,
    Drop {
        name: String,
    },
}

async fn serve_web_graph(store: &Store, port: u16) -> Result<()> {
    // Find lupe-server binary next to this binary, or on PATH
    let server_bin = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("lupe-server")))
        .filter(|p| p.exists())
        .or_else(|| which::which("lupe-server").ok())
        .ok_or_else(|| anyhow!("lupe-server not found — install it alongside lupe"))?;

    let home_arg = store.home.display().to_string();
    let mut child = tokio::process::Command::new(&server_bin)
        .args(["--home", &home_arg, "--port", &port.to_string()])
        .spawn()?;

    // Wait briefly for server to start
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

    let url = format!("http://localhost:{port}");
    println!("lupe graph — opening {url}");
    println!("ctrl+c to stop");
    let _ = open::that(&url);

    // Wait for server process (or Ctrl+C)
    child.wait().await?;
    Ok(())
}

// ── main ──────────────────────────────────────────────────────────────────────

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
            session,
            private,
        } => {
            if let Some(response) = prev_response {
                if let Ok(prev_id) = store.latest_checkpoint_id().await {
                    let _ = store.set_response_for(prev_id, response).await;
                }
            }
            let workspace = absolutize(workspace)?;
            write_default_lupeignore(&workspace)?;
            let title = title.unwrap_or_else(|| title_from_prompt(&prompt));
            let agent = detect_agent(agent);
            let checkpoint = store
                .create_checkpoint(title, Some(prompt), Some(agent), session, &workspace, private)
                .await?;
            println!(
                "checkpoint {} ({}) files={} root={} {}",
                short_id(checkpoint.id),
                checkpoint.id,
                checkpoint.file_count,
                &checkpoint.root_hash[..12],
                checkpoint.title
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
            let checkpoint = store
                .create_checkpoint(title, Some(prompt), Some(agent), None, &workspace, false)
                .await?;
            println!(
                "checkpoint {} ({}) files={} root={} {}",
                short_id(checkpoint.id),
                checkpoint.id,
                checkpoint.file_count,
                &checkpoint.root_hash[..12],
                checkpoint.title
            );
        }
        Command::Save { message, workspace } => {
            let workspace = absolutize(workspace)?;
            let title = message.unwrap_or_else(|| "save".to_string());
            let checkpoint = store
                .create_checkpoint(title.clone(), None, None, None, &workspace, false)
                .await?;
            println!(
                "checkpoint {} ({}) files={} root={} {}",
                short_id(checkpoint.id),
                checkpoint.id,
                checkpoint.file_count,
                &checkpoint.root_hash[..12],
                checkpoint.title
            );
        }
        Command::History { all, show_private } => {
            for checkpoint in store.list_checkpoints(all, show_private).await? {
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
                println!(
                    "  prompt: {}",
                    one_line(checkpoint.prompt.as_deref().unwrap_or(""))
                );
            }
        }
        Command::Prompts { all, show_private } => {
            for checkpoint in store.list_checkpoints(all, show_private).await? {
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
        Command::Graph { no_color, all, show_private, web, port } => {
            if web {
                serve_web_graph(&store, port).await?;
                return Ok(());
            }
            let colors = Colors::new(!no_color);
            let head_checkpoint = store.read_head();

            // Main chain always shows only main-branch checkpoints.
            let checkpoints = store.list_checkpoints(false, show_private).await?;
            if checkpoints.is_empty() {
                println!("no checkpoints yet");
                return Ok(());
            }

            // Build maps for --all display:
            //   branch_children: parent_id → direct non-main children (branch entry points)
            //   all_children:    parent_id → ALL children (to follow full branch chains)
            let mut branch_children: std::collections::HashMap<Uuid, Vec<lupe_core::CheckpointView>> =
                std::collections::HashMap::new();
            let mut all_children: std::collections::HashMap<Uuid, Vec<lupe_core::CheckpointView>> =
                std::collections::HashMap::new();
            if all {
                let all_checkpoints = store.list_checkpoints(true, show_private).await?;
                for c in all_checkpoints {
                    if let Some(pcid) = c.parent_checkpoint_id {
                        all_children.entry(pcid).or_default().push(c.clone());
                        if c.branch_name != "main" {
                            branch_children.entry(pcid).or_default().push(c);
                        }
                    }
                }
            }

            for (index, checkpoint) in checkpoints.iter().rev().enumerate() {
                if index > 0 {
                    println!("{}", colors.dim("│"));
                }
                let is_head = head_checkpoint == Some(checkpoint.id);
                let head_marker = if is_head { " [HEAD]" } else { "" };
                let (cp_label, title_display, pipe, prompt_label, checkpoint_sym) =
                    if checkpoint.private {
                        let title = if show_private {
                            colors.private_cp(&checkpoint.title)
                        } else {
                            colors.private_cp("[private]")
                        };
                        (
                            colors.private_cp(&format!("checkpoint {}", short_id(checkpoint.id))),
                            title,
                            colors.private_cp("│"),
                            colors.private_cp("prompt:"),
                            colors.private_cp("◆"),
                        )
                    } else {
                        (
                            colors.checkpoint(&format!("checkpoint {}", short_id(checkpoint.id))),
                            colors.bold(&checkpoint.title),
                            colors.dim("│"),
                            colors.dim("prompt:"),
                            colors.checkpoint("◆"),
                        )
                    };
                println!(
                    "{} {} {} {} {} files={} root={}{}",
                    checkpoint_sym,
                    cp_label,
                    colors.dim(&format!("({})", checkpoint.id)),
                    colors.dim(&friendly_time(checkpoint.created_at)),
                    title_display,
                    checkpoint.file_count,
                    &checkpoint.root_hash[..12],
                    head_marker,
                );
                let prompt_display = if checkpoint.private && !show_private {
                    "[private]".to_string()
                } else {
                    one_line(checkpoint.prompt.as_deref().unwrap_or(""))
                };
                println!("{} {} {}", pipe, prompt_label, prompt_display);
                if !checkpoint.private || show_private {
                    if let Some(agent) = &checkpoint.agent {
                        let session_suffix = checkpoint.session_id.as_deref()
                            .map(|s| format!("  {}", colors.dim(&format!("session: {}", s))))
                            .unwrap_or_default();
                        println!("{} {} {}{}", colors.dim("│"), colors.dim("agent:"), agent, session_suffix);
                    }
                }

                if let Some(parent_id) = checkpoint.parent_checkpoint_id {
                    let diff = store.diff_checkpoints(parent_id, checkpoint.id).await?;
                    let total = diff.added.len() + diff.modified.len() + diff.removed.len();
                    if total > 0 {
                        println!(
                            "{} {} +{} ~{} -{}",
                            colors.dim("│"),
                            colors.dim("changes:"),
                            diff.added.len(),
                            diff.modified.len(),
                            diff.removed.len(),
                        );
                        print_diff_lines(&diff, &colors, &colors.dim("│"), "  ");
                    }
                }

                if let Some(children) = branch_children.get(&checkpoint.id) {
                    for (i, entry) in children.iter().enumerate() {
                        let is_last_branch = i + 1 == children.len();
                        let branch_pipe = if is_last_branch { " " } else { "│" };

                        // Walk the full chain for this branch from the entry point.
                        let mut chain = vec![entry.clone()];
                        let mut tip = entry.id;
                        loop {
                            let next: Vec<_> = all_children
                                .get(&tip)
                                .map(|v| v.iter().filter(|c| c.branch_name == entry.branch_name).collect())
                                .unwrap_or_default();
                            if next.is_empty() { break; }
                            let n = next[0].clone();
                            tip = n.id;
                            chain.push(n);
                        }

                        for (j, node) in chain.iter().enumerate() {
                            let is_first = j == 0;
                            let indent = if is_first {
                                format!("{branch_pipe}  ")
                            } else {
                                format!("{branch_pipe}     ")
                            };
                            println!("{}", colors.dim(&format!("{indent}│")));
                            let connector = if is_first { "╰─ ◆" } else { "   ◆" };
                            let branch_tag = if is_first {
                                format!("branch: {} ", node.branch_name)
                            } else {
                                String::new()
                            };
                            println!(
                                "{}",
                                colors.branch(&format!(
                                    "{indent}{connector} {}{} ({}) {} files={} root={}",
                                    branch_tag,
                                    short_id(node.id),
                                    node.id,
                                    node.title,
                                    node.file_count,
                                    &node.root_hash[..12],
                                ))
                            );
                            if let Some(p) = node.prompt.as_deref().filter(|s| !s.is_empty()) {
                                println!(
                                    "{}",
                                    colors.branch(&format!(
                                        "{}         prompt: {}",
                                        indent,
                                        one_line(p)
                                    ))
                                );
                            }
                        }
                    }
                }
            }
        }
        Command::Diff { from, to } => {
            let (from, to) = store.resolve_diff_range(from, to).await?;
            let diff = store.diff_checkpoints(from, to).await?;
            println!("from {} ({from})", short_id(from));
            println!("to   {} ({to})", short_id(to));
            print_path_list("added", &diff.added);
            print_path_list("modified", &diff.modified);
            print_path_list("removed", &diff.removed);
        }
        Command::Restore { checkpoint, workspace } => {
            let workspace = absolutize(workspace)?;
            let checkpoint_id = match Uuid::parse_str(&checkpoint) {
                Ok(id) => id,
                Err(_) => store.resolve_branch_name(&checkpoint).await?,
            };
            let cp = store.restore_checkpoint(checkpoint_id, &workspace).await?;
            println!(
                "restored checkpoint {} ({}) files={}",
                short_id(cp.id),
                cp.id,
                cp.file_count
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
                        .list_checkpoints(false, false)
                        .await?
                        .into_iter()
                        .next()
                        .ok_or_else(|| anyhow!("no checkpoint found; run `lupe prompt` first"))?;
                    checkpoint.title
                }
            };
            git_push(&workspace, &msg)?;
        }
        Command::Install {
            workspace,
            lupe_bin,
            no_agent,
            no_hooks,
        } => {
            if no_agent && no_hooks {
                bail!("nothing to install: remove either --no-agent or --no-hooks");
            }
            let workspace = absolutize(workspace)?;
            if !no_agent {
                let path = install_agent_instructions(&workspace)?;
                println!("agent       -> {}", path.display());
            }
            if !no_hooks {
                let lupe_bin = resolve_lupe_bin(lupe_bin)?;
                install_hooks(&lupe_bin)?;
            }
            println!("done. restart your agents to pick up new hooks.");
        }
        Command::InstallHooks { lupe_bin } => {
            let lupe_bin = resolve_lupe_bin(lupe_bin)?;
            install_hooks(&lupe_bin)?;
            println!("done. restart your agents to pick up new hooks.");
        }
        Command::InstallAgent { workspace } => {
            let workspace = absolutize(workspace)?;
            let path = install_agent_instructions(&workspace)?;
            println!("installed agent instructions {}", path.display());
        }
        Command::Status | Command::Init => {
            let workspace = absolutize(PathBuf::from("."))?;
            write_default_lupeignore(&workspace)?;
            install_agent_instructions(&workspace)?;
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
        Command::Title { title } => {
            let checkpoint_id = store.set_title(title.clone()).await?;
            println!("title updated {} -> {}", short_id(checkpoint_id), title);
        }
        Command::Branch { name } => {
            let branch = store.create_branch(name).await?;
            println!("branch {} -> checkpoint {}", branch.name, short_id(branch.head_checkpoint_id));
        }
        Command::Branches => {
            let branches = store.list_branches().await?;
            if branches.is_empty() {
                println!("no branches — run: lupe branch <name>");
            } else {
                for branch in branches {
                    println!(
                        "{}  head={}  {}",
                        branch.name,
                        short_id(branch.head_checkpoint_id),
                        friendly_time(branch.updated_at),
                    );
                }
            }
        }
        Command::Workspace { action } => match action {
            WorkspaceAction::New { branch, workspace } => {
                let workspace = absolutize(workspace)?;
                let ws_dir = store.create_workspace(&branch, &workspace).await?;
                println!("workspace '{branch}' created");
                println!("path  {}", ws_dir.display());
                println!("cd into it and run your app independently");
            }
            WorkspaceAction::List => {
                let workspaces = store.list_workspaces()?;
                if workspaces.is_empty() {
                    println!("no workspaces — run: lupe workspace new <branch-name>");
                } else {
                    for ws in workspaces {
                        println!("{}  branch={}  {}", ws.name, ws.branch, ws.path.display());
                    }
                }
            }
            WorkspaceAction::Drop { name } => {
                store.drop_workspace(&name)?;
                println!("workspace '{name}' dropped");
            }
        },
        Command::Cat { file, checkpoint } => {
            let checkpoint_id = store.resolve_checkpoint_id(&checkpoint).await?;
            let manifest = store.get_manifest(checkpoint_id).await?;
            let entry = manifest.files.iter().find(|f| f.path == file)
                .ok_or_else(|| anyhow!("file '{}' not found in checkpoint {}", file, short_id(checkpoint_id)))?;
            let obj = lupe_core::object_path(&store.object_dir, &entry.hash)?;
            let content = std::fs::read_to_string(&obj)
                .map_err(|e| anyhow!("failed to read object {}: {}", entry.hash, e))?;
            print!("{content}");
        }
        Command::Files { checkpoint } => {
            let checkpoint_id = store.resolve_checkpoint_id(&checkpoint).await?;
            let manifest = store.get_manifest(checkpoint_id).await?;
            for f in &manifest.files {
                println!("{}", f.path);
            }
        }
        Command::Private => {
            store.set_next_private()?;
            println!("next checkpoint will be private");
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
                    println!(
                        "set with: lupe author --name \"Your Name\" --email \"your@email.com\""
                    );
                }
            }
        }
        Command::Docs => {
            print!("{}", DOCS);
        }
        Command::Use { branch } => {
            let cwd = std::env::current_dir()?;
            let branch_file = cwd.join(".lupe-branch");
            if branch == "main" {
                // Remove .lupe-branch so the directory reverts to main
                if branch_file.exists() {
                    std::fs::remove_file(&branch_file)?;
                }
                println!("now on main — .lupe-branch removed");
            } else {
                // Verify branch exists before committing
                let branches = store.list_branches().await?;
                if !branches.iter().any(|b| b.name == branch) {
                    anyhow::bail!("branch '{}' not found — run: lupe branch {}", branch, branch);
                }
                std::fs::write(&branch_file, &branch)?;
                println!("now on {} — checkpoints will go to this branch", branch);
            }
        }
    }

    Ok(())
}

fn print_path_list(label: &str, paths: &[String]) {
    println!("{label}: {}", paths.len());
    for path in paths {
        println!("  {path}");
    }
}
