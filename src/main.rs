use anyhow::Result;
use clap::{Parser, Subcommand};
use meetings::*;

#[derive(Parser)]
#[command(
    name = "meetings",
    about = "CLI for querying Anarlog/hyprnote meeting sessions"
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
    /// List sessions, optionally filtered by title substring
    List {
        /// Case-insensitive title or ID substring filter
        #[arg(long)]
        query: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show session details: note, summaries, action items
    Show {
        /// Session ID (or case-insensitive title substring)
        id: String,
    },
    /// List meetings from the same recurring series
    History {
        /// Session ID (or title substring) whose series to look up
        id: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Check database connection and schema
    Doctor,
    /// Run the read-only MCP server over stdio
    Mcp,
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
        Commands::List { query, json } => {
            let mut entries = models::load_sessions(&db_path)?;

            if let Some(q) = &query {
                let ql = q.to_lowercase();
                entries.retain(|s| {
                    s.meta.title.to_lowercase().contains(&ql)
                        || s.id.to_lowercase().contains(&ql)
                });
            }

            if json {
                let out: Vec<serde_json::Value> = entries
                    .iter()
                    .map(|s| {
                        serde_json::json!({
                            "id": s.id,
                            "title": s.meta.title,
                            "kind": s.meta.kind,
                            "status": s.meta.status,
                            "created_at": s.meta.created_at,
                            "updated_at": s.meta.updated_at,
                            "started_at": s.meta.started_at,
                            "ended_at": s.meta.ended_at,
                            "series_id": s.meta.series_id,
                            "participants": s.meta.participants.len(),
                            "has_note": !s.note.is_empty(),
                            "summary_count": s.summaries.len(),
                            "action_item_count": s.action_items.len(),
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
                    if !s.meta.series_id.is_empty() {
                        println!("  Series:  {}", s.meta.series_id);
                    }
                    println!();
                }
            }
        }
        Commands::Show { id } => {
            let entries = models::load_sessions(&db_path)?;
            let session = find_session(&entries, &id)?;

            println!("=== {} ===", session.meta.title);
            println!("ID: {}", session.id);
            println!("Created: {}", session.meta.created_at);
            println!();

            if !session.note.is_empty() {
                println!("--- Note ---\n");
                println!("{}", session.note);
                println!();
            }

            for summary in &session.summaries {
                let label = if summary.title.is_empty() {
                    "Summary"
                } else {
                    &summary.title
                };
                println!("--- {} ---\n", label);
                println!("{}", summary.markdown);
                println!();
            }

            if !session.action_items.is_empty() {
                println!("--- Action Items ---\n");
                for item in &session.action_items {
                    let check = if item.status == "done" {
                        "x"
                    } else {
                        " "
                    };
                    println!("[{}] {}", check, item.text);
                }
                println!();
            }
        }
        Commands::History { id, json } => {
            let entries = models::load_sessions(&db_path)?;
            let session = find_session(&entries, &id)?;

            if session.meta.series_id.is_empty() {
                eprintln!(
                    "Session '{}' has no recurring series.",
                    session.meta.title
                );
                return Ok(());
            }

            let series: Vec<_> = entries
                .iter()
                .filter(|s| s.meta.series_id == session.meta.series_id)
                .collect();

            if json {
                let out: Vec<serde_json::Value> = series
                    .iter()
                    .map(|s| {
                        serde_json::json!({
                            "id": s.id,
                            "title": s.meta.title,
                            "created_at": s.meta.created_at,
                            "series_id": s.meta.series_id,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                println!("Series: {}", session.meta.series_id);
                for s in &series {
                    println!(
                        "  {}  {}  ({})",
                        s.id, s.meta.title, s.meta.created_at
                    );
                }
            }
        }
        Commands::Doctor => {
            run_doctor(&db_path)?;
        }
        Commands::Mcp => {
            mcp::serve(db_path)?;
        }
        Commands::Index { segment_ms } => {
            index::run_index(&db_path, segment_ms)?;
        }
        Commands::Search { query, top_k, json } => {
            search::run_search(&query, top_k, json)?;
        }
        Commands::Speakers { id, pause_ms, json } => {
            let entries = models::load_sessions(&db_path)?;
            let session = find_session(&entries, &id)?;

            let utterances = session.utterances(pause_ms);

            if json {
                println!("{}", serde_json::to_string_pretty(&utterances)?);
            } else {
                for u in &utterances {
                    let mins = u.start_ms / 60000;
                    let secs = (u.start_ms % 60000) / 1000;
                    println!(
                        "[{:02}:{:02}] {}: {}",
                        mins, secs, u.speaker_label, u.text
                    );
                }
            }
        }
    }

    Ok(())
}

/// Find a session by exact ID or case-insensitive title substring.
fn find_session<'a>(
    entries: &'a [models::Session],
    id: &str,
) -> Result<&'a models::Session> {
    entries
        .iter()
        .find(|s| s.id == id || s.meta.title.to_lowercase().contains(&id.to_lowercase()))
        .ok_or_else(|| anyhow::anyhow!("Session not found: {id}"))
}

/// Check database path, read access, and schema.
fn run_doctor(db_path: &std::path::Path) -> Result<()> {
    let mut ok = true;

    print!("Database path:  {}  ", db_path.display());
    if db_path.exists() {
        println!("[ok] exists");
    } else {
        println!("[FAIL] not found");
        ok = false;
    }

    if ok {
        print!("Load sessions:  ");
        match models::load_sessions(db_path) {
            Ok(sessions) => println!("[ok] {} sessions", sessions.len()),
            Err(e) => {
                println!("[FAIL] {e}");
                ok = false;
            }
        }
    }

    println!();
    if ok {
        println!("Ready: yes");
    } else {
        println!("Ready: no");
        std::process::exit(1);
    }
    Ok(())
}
