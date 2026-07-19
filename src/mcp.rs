use anyhow::Result;
use rmcp::{
    handler::server::wrapper::Parameters,
    schemars,
    service::ServiceExt,
    tool, tool_router,
    transport::io::stdio,
};
use serde::Deserialize;
use std::path::PathBuf;

use crate::index::ensure_fresh_index;
use crate::models;
use crate::search;

struct MeetingServer {
    db_path: PathBuf,
}

// --- Tool parameter structs ---

#[derive(Deserialize, schemars::JsonSchema)]
struct ListMeetingsParam {
    /// Optional case-insensitive title or ID substring filter.
    #[serde(default)]
    query: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct GetMeetingParam {
    /// Meeting ID or case-insensitive title substring.
    meeting_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct SearchParam {
    /// Natural-language search query.
    query: String,
    /// Max results to return (default 5).
    #[serde(default)]
    top_k: Option<usize>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct GetTranscriptParam {
    /// Meeting ID or case-insensitive title substring.
    meeting_id: String,
    /// Pause threshold in ms for splitting utterances (default 1500).
    #[serde(default)]
    pause_ms: Option<i64>,
}

#[tool_router(server_handler)]
impl MeetingServer {
    #[tool(description = "List meetings from the local Anarlog/hyprnote database. Returns id, title, dates, series_id, and document counts. Optionally filter by title substring.")]
    fn list_meetings(
        &self,
        Parameters(ListMeetingsParam { query }): Parameters<ListMeetingsParam>,
    ) -> String {
        let sessions = match models::load_sessions(&self.db_path) {
            Ok(s) => s,
            Err(e) => return format!("Error loading sessions: {e}"),
        };

        let filtered: Vec<_> = match &query {
            Some(q) => {
                let ql = q.to_lowercase();
                sessions
                    .iter()
                    .filter(|s| {
                        s.meta.title.to_lowercase().contains(&ql)
                            || s.id.to_lowercase().contains(&ql)
                    })
                    .collect()
            }
            None => sessions.iter().collect(),
        };

        let out: Vec<serde_json::Value> = filtered
            .iter()
            .map(|s| {
                serde_json::json!({
                    "id": s.id,
                    "title": s.meta.title,
                    "created_at": s.meta.created_at,
                    "series_id": s.meta.series_id,
                    "has_note": !s.note.is_empty(),
                    "summary_count": s.summaries.len(),
                    "action_item_count": s.action_items.len(),
                    "participants": s.meta.participants.len(),
                })
            })
            .collect();

        serde_json::to_string_pretty(&out).unwrap_or_else(|_| "[]".into())
    }

    #[tool(description = "Get full meeting details: note, AI summaries, action items, and participants. Pass a meeting ID or title substring.")]
    fn get_meeting(
        &self,
        Parameters(GetMeetingParam { meeting_id }): Parameters<GetMeetingParam>,
    ) -> String {
        let sessions = match models::load_sessions(&self.db_path) {
            Ok(s) => s,
            Err(e) => return format!("Error loading sessions: {e}"),
        };

        let session = match find_session(&sessions, &meeting_id) {
            Some(s) => s,
            None => return format!("Meeting not found: {meeting_id}"),
        };

        let summaries: Vec<serde_json::Value> = session
            .summaries
            .iter()
            .map(|d| {
                serde_json::json!({
                    "title": d.title,
                    "markdown": d.markdown,
                })
            })
            .collect();

        let action_items: Vec<serde_json::Value> = session
            .action_items
            .iter()
            .map(|a| {
                serde_json::json!({
                    "text": a.text,
                    "status": a.status,
                    "due_at": a.due_at,
                })
            })
            .collect();

        let participants: Vec<&str> = session
            .meta
            .participants
            .iter()
            .map(|p| p.display_name.as_str())
            .filter(|n| !n.is_empty())
            .collect();

        let result = serde_json::json!({
            "id": session.id,
            "title": session.meta.title,
            "created_at": session.meta.created_at,
            "note": session.note,
            "summaries": summaries,
            "action_items": action_items,
            "participants": participants,
        });

        serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".into())
    }

    #[tool(description = "Semantic search across all meeting notes, summaries, and transcripts using local embeddings. Auto-indexes if the index is stale. Returns ranked chunks with similarity scores.")]
    fn search_sessions(
        &self,
        Parameters(SearchParam { query, top_k }): Parameters<SearchParam>,
    ) -> String {
        eprintln!("[mcp] ensuring fresh index...");
        if let Err(e) = ensure_fresh_index(&self.db_path) {
            return format!("Error building search index: {e}");
        }

        let k = top_k.unwrap_or(5);
        match search::perform_search(&query, k) {
            Ok(results) => serde_json::to_string_pretty(&results)
                .unwrap_or_else(|_| "[]".into()),
            Err(e) => format!("Search error: {e}"),
        }
    }

    #[tool(description = "Get a speaker-segmented transcript for a meeting. Returns utterances with timestamps and speaker labels. Pass a meeting ID or title substring.")]
    fn get_transcript(
        &self,
        Parameters(GetTranscriptParam {
            meeting_id,
            pause_ms,
        }): Parameters<GetTranscriptParam>,
    ) -> String {
        let sessions = match models::load_sessions(&self.db_path) {
            Ok(s) => s,
            Err(e) => return format!("Error loading sessions: {e}"),
        };

        let session = match find_session(&sessions, &meeting_id) {
            Some(s) => s,
            None => return format!("Meeting not found: {meeting_id}"),
        };

        let utterances = session.utterances(pause_ms.unwrap_or(1500));
        serde_json::to_string_pretty(&utterances).unwrap_or_else(|_| "[]".into())
    }
}

/// Run the MCP server over stdio.
pub fn serve(db_path: PathBuf) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let server = MeetingServer { db_path };
        let service = server.serve(stdio()).await?;
        service.waiting().await?;
        Ok::<(), anyhow::Error>(())
    })
}

/// Find a session by exact ID or case-insensitive title substring.
fn find_session<'a>(entries: &'a [models::Session], id: &str) -> Option<&'a models::Session> {
    let id_lower = id.to_lowercase();
    entries
        .iter()
        .find(|s| s.id == id || s.meta.title.to_lowercase().contains(&id_lower))
}
