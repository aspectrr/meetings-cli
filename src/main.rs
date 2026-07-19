use anyhow::Result;
use clap::{Parser, Subcommand};
use meetings::*;

#[derive(Parser)]
#[command(
    name = "meetings",
    about = "CLI for querying hyprnote meeting sessions"
)]
struct Cli {
    /// Path to the Anarlog/hyprnote app.db database
    #[arg(long, global = true)]
    db_path: Option<String>,

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
    let db_path = match cli.db_path.as_deref() {
        Some(p) => std::path::PathBuf::from(p),
        None => models::default_db_path()?,
    };

    match cli.command {
        Commands::List { json } => {
            let entries = models::load_sessions(&db_path)?;

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
            index::run_index(&db_path, segment_ms)?;
        }
        Commands::Search { query, top_k, json } => {
            search::run_search(&query, top_k, json)?;
        }
        Commands::Show { id } => {
            let entries = models::load_sessions(&db_path)?;
            let session = entries
                .iter()
                .find(|s| {
                    s.id == id || s.meta.title.to_lowercase().contains(&id.to_lowercase())
                })
                .ok_or_else(|| anyhow::anyhow!("Session not found: {id}"))?;

            println!("=== {} ===", session.meta.title);
            println!("ID: {}", session.id);
            println!("Created: {}", session.meta.created_at);
            println!();
            println!("{}", session.memo);
        }
        Commands::Speakers { id, pause_ms, json } => {
            let entries = models::load_sessions(&db_path)?;
            let session = entries
                .iter()
                .find(|s| {
                    s.id == id || s.meta.title.to_lowercase().contains(&id.to_lowercase())
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
