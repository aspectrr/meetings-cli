use anyhow::Result;
use clap::{Parser, Subcommand};
use meetings::*;

#[derive(Parser)]
#[command(
    name = "meetings",
    about = "CLI for querying hyprnote meeting sessions"
)]
struct Cli {
    /// Path to hyprnote sessions directory
    #[arg(long, global = true)]
    sessions_path: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all sessions
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Index sessions for semantic search
    Index {
        /// Segment duration in milliseconds for transcript chunking
        #[arg(long, default_value = "60000")]
        segment_ms: i64,
    },
    /// Semantic search across indexed sessions
    Search {
        /// Search query
        query: String,
        /// Number of results
        #[arg(long, default_value = "5")]
        top_k: usize,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show session memo
    Show {
        /// Session ID (or substring match on title)
        id: String,
    },
    /// List utterances (speaker segments) from a session
    Speakers {
        /// Session ID (or substring match on title)
        id: String,
        /// Pause threshold in ms for splitting utterances
        #[arg(long, default_value = "1500")]
        pause_ms: i64,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::List { json } => {
            let sp = cli.sessions_path.as_deref().map(std::path::Path::new);
            let sp = match sp {
                Some(p) => p.to_path_buf(),
                None => models::default_sessions_path()?,
            };
            let dirs = models::session_dirs(&sp)?;
            let mut entries = Vec::new();
            for d in &dirs {
                match models::Session::load(d) {
                    Ok(s) => entries.push(s),
                    Err(e) => eprintln!("Error loading {}: {e}", d.display()),
                }
            }

            if json {
                let out: Vec<serde_json::Value> = entries
                    .iter()
                    .map(|s| {
                        serde_json::json!({
                            "id": s.id,
                            "title": s.meta.title,
                            "created_at": s.meta.created_at,
                            "participants": s.meta.participants.len(),
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                for s in &entries {
                    println!(
                        "{}  {}  ({} participants)",
                        s.id,
                        s.meta.title,
                        s.meta.participants.len()
                    );
                    println!("  Created: {}", s.meta.created_at);
                    println!();
                }
            }
        }
        Commands::Index { segment_ms } => {
            let sp = cli.sessions_path.as_deref().map(std::path::Path::new);
            index::run_index(sp, segment_ms)?;
        }
        Commands::Search { query, top_k, json } => {
            search::run_search(&query, top_k, json)?;
        }
        Commands::Show { id } => {
            let sp = cli.sessions_path.as_deref().map(std::path::Path::new);
            let sp = match sp {
                Some(p) => p.to_path_buf(),
                None => models::default_sessions_path()?,
            };
            let dirs = models::session_dirs(&sp)?;
            let session = dirs
                .iter()
                .find_map(|d| {
                    let s = models::Session::load(d).ok()?;
                    if s.id == id || s.meta.title.to_lowercase().contains(&id.to_lowercase()) {
                        Some(s)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| anyhow::anyhow!("Session not found: {id}"))?;

            println!("=== {} ===", session.meta.title);
            println!("ID: {}", session.id);
            println!("Created: {}", session.meta.created_at);
            println!();
            println!("{}", session.memo);
        }
        Commands::Speakers { id, pause_ms, json } => {
            let sp = cli.sessions_path.as_deref().map(std::path::Path::new);
            let sp = match sp {
                Some(p) => p.to_path_buf(),
                None => models::default_sessions_path()?,
            };
            let dirs = models::session_dirs(&sp)?;
            let session = dirs
                .iter()
                .find_map(|d| {
                    let s = models::Session::load(d).ok()?;
                    if s.id == id || s.meta.title.to_lowercase().contains(&id.to_lowercase()) {
                        Some(s)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| anyhow::anyhow!("Session not found: {id}"))?;

            let utterances = session.utterances(pause_ms);

            if json {
                println!("{}", serde_json::to_string_pretty(&utterances)?);
            } else {
                for u in &utterances {
                    let mins = u.start_ms / 60000;
                    let secs = (u.start_ms % 60000) / 1000;
                    println!("[{:02}:{:02}] {}: {}", mins, secs, u.speaker_label, u.text);
                }
            }
        }
    }

    Ok(())
}
