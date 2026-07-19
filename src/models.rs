use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Session metadata (loaded from app.db)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    pub created_at: String,
    pub updated_at: String,
    pub id: String,
    pub title: String,
    pub kind: String,
    pub status: String,
    pub started_at: String,
    pub ended_at: String,
    pub series_id: String,
    pub participants: Vec<Participant>,
}

/// A note or summary document from `session_documents`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub markdown: String,
    pub sort_order: i64,
}

/// An action item extracted from a meeting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    pub id: String,
    pub text: String,
    pub status: String,
    pub assignee_human_id: String,
    pub due_at: String,
    pub completed_at: Option<String>,
    pub source_order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    pub id: String,
    pub session_id: String,
    pub human_id: String,
    pub display_name: String,
    pub email: String,
    pub role: String,
    pub source: String,
}

/// Transcript from transcript.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptFile {
    pub transcripts: Vec<Transcript>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub id: String,
    pub session_id: String,
    pub words: Vec<Word>,
    #[serde(default)]
    pub speaker_hints: Vec<serde_json::Value>,
    #[serde(default)]
    pub started_at: i64,
    // ponytail: memo column dropped — notes now read from session_documents
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Word {
    pub text: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub channel: i32,
    #[serde(default)]
    pub id: String,
}

/// A loaded session with all data
#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub path: PathBuf,
    pub meta: Meta,
    pub note: String,
    pub summaries: Vec<Document>,
    pub action_items: Vec<ActionItem>,
    pub transcript: Option<TranscriptFile>,
}

/// A text chunk for embedding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub session_id: String,
    pub title: String,
    pub chunk_type: ChunkType,
    pub text: String,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub channel: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChunkType {
    Note,
    Summary,
    TranscriptSegment,
}

/// An utterance grouped by channel/time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Utterance {
    pub channel: i32,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub speaker_label: String,
}

impl Session {

    /// Extract utterances from transcript (group words by channel with pause detection)
    pub fn utterances(&self, pause_threshold_ms: i64) -> Vec<Utterance> {
        let Some(tf) = &self.transcript else {
            return vec![];
        };
        let mut all_utterances = vec![];
        for t in &tf.transcripts {
            let words = &t.words;
            if words.is_empty() {
                continue;
            }

            let mut chunks: Vec<Vec<&Word>> = vec![];
            let mut current = vec![&words[0]];

            for w in &words[1..] {
                let prev = current.last().unwrap();
                let gap = w.start_ms - prev.end_ms;
                if w.channel != prev.channel || gap > pause_threshold_ms {
                    chunks.push(std::mem::take(&mut current));
                    current = vec![w];
                } else {
                    current.push(w);
                }
            }
            if !current.is_empty() {
                chunks.push(current);
            }

            for chunk in chunks {
                let text: String = chunk
                    .iter()
                    .map(|w| w.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                let start = chunk.first().unwrap().start_ms;
                let end = chunk.last().unwrap().end_ms;
                let channel = chunk[0].channel;
                all_utterances.push(Utterance {
                    channel,
                    start_ms: start,
                    end_ms: end,
                    text: text.trim().to_string(),
                    speaker_label: format!("Speaker {channel}"),
                });
            }
        }
        all_utterances
    }

    /// Build chunks for embedding: memo + transcript segments
    pub fn chunks(&self, segment_duration_ms: i64) -> Vec<Chunk> {
        let mut chunks = vec![];

        // Note as one chunk
        if !self.note.is_empty() {
            chunks.push(Chunk {
                session_id: self.id.clone(),
                title: self.meta.title.clone(),
                chunk_type: ChunkType::Note,
                text: self.note.clone(),
                start_ms: None,
                end_ms: None,
                channel: None,
            });
        }

        // Summaries as chunks
        for s in &self.summaries {
            if !s.markdown.is_empty() {
                chunks.push(Chunk {
                    session_id: self.id.clone(),
                    title: self.meta.title.clone(),
                    chunk_type: ChunkType::Summary,
                    text: s.markdown.clone(),
                    start_ms: None,
                    end_ms: None,
                    channel: None,
                });
            }
        }

        // Transcript split into segments by time
        if let Some(tf) = &self.transcript {
            for t in &tf.transcripts {
                let words = &t.words;
                if words.is_empty() {
                    continue;
                }
                let seg_len = segment_duration_ms;
                let mut seg_start = words[0].start_ms;
                let mut seg_words: Vec<&Word> = vec![];

                for w in words {
                    if w.start_ms - seg_start >= seg_len && !seg_words.is_empty() {
                        let text: String = seg_words
                            .iter()
                            .map(|w| w.text.as_str())
                            .collect::<Vec<_>>()
                            .join(" ");
                        if !text.trim().is_empty() {
                            chunks.push(Chunk {
                                session_id: self.id.clone(),
                                title: self.meta.title.clone(),
                                chunk_type: ChunkType::TranscriptSegment,
                                text: text.trim().to_string(),
                                start_ms: Some(seg_start),
                                end_ms: Some(seg_words.last().unwrap().end_ms),
                                channel: None,
                            });
                        }
                        seg_start = w.start_ms;
                        seg_words = vec![w];
                    } else {
                        seg_words.push(w);
                    }
                }
                // Flush remaining
                if !seg_words.is_empty() {
                    let text: String = seg_words
                        .iter()
                        .map(|w| w.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");
                    if !text.trim().is_empty() {
                        chunks.push(Chunk {
                            session_id: self.id.clone(),
                            title: self.meta.title.clone(),
                            chunk_type: ChunkType::TranscriptSegment,
                            text: text.trim().to_string(),
                            start_ms: Some(seg_start),
                            end_ms: Some(seg_words.last().unwrap().end_ms),
                            channel: None,
                        });
                    }
                }
            }
        }

        chunks
    }
}

/// Default location of the Anarlog (formerly hyprnote) SQLite database.
pub fn default_db_path() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("No home directory"))?;
    Ok(home.join("Library/Application Support/hyprnote/app.db"))
}

/// Load every (non-deleted) session from the Anarlog SQLite database.
///
/// Replaces the old flat-file reader (`_meta.json` / `_memo.md` / `transcript.json`):
/// newer Anarlog versions write everything to `app.db` and stop emitting those
/// files, so a session folder can contain only `audio.mp3` (e.g. the CDL Intro
/// Meeting) yet be fully present in the DB.
pub fn load_sessions(db_path: &std::path::Path) -> anyhow::Result<Vec<Session>> {
    // immutable=1: treat the live app DB as read-only snapshot so we never need
    // write access to the -wal/-shm files the app may hold open.
    let abs = db_path
        .canonicalize()
        .unwrap_or_else(|_| db_path.to_path_buf());
    let uri = format!("file:{}?immutable=1", abs.display());
    let conn = Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    // Transcripts (words + speaker hints), grouped by session_id.
    let mut transcripts_by_session: HashMap<String, Vec<Transcript>> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT session_id, id, started_at_ms, words_json, speaker_hints_json
         FROM transcripts
         WHERE deleted_at IS NULL",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let session_id: String = row.get(0)?;
        let id: String = row.get(1)?;
        let started_at: i64 = row.get::<_, Option<i64>>(2)?.unwrap_or(0);
        let words_json: String = row.get(3)?;
        let speaker_json: String = row.get(4)?;

        let words: Vec<Word> = serde_json::from_str(&words_json).unwrap_or_default();
        let speaker_hints: Vec<serde_json::Value> =
            serde_json::from_str(&speaker_json).unwrap_or_default();

        transcripts_by_session.entry(session_id).or_default().push(Transcript {
            id,
            session_id: String::new(),
            words,
            speaker_hints,
            started_at,
        });
    }
    drop(rows);
    drop(stmt);

    // Notes + summaries from session_documents, grouped by session_id.
    let mut docs_by_session: HashMap<String, (Option<Document>, Vec<Document>)> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT id, session_id, kind, title, body, body_format, sort_order
         FROM session_documents
         WHERE deleted_at IS NULL
         ORDER BY sort_order",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let session_id: String = row.get(1)?;
        let kind: String = row.get(2)?;
        let title: String = row.get(3)?;
        let body: String = row.get(4)?;
        let body_format: String = row.get(5)?;
        let sort_order: i64 = row.get(6)?;

        let markdown = body_to_text(&body, &body_format);
        let doc = Document { id, kind: kind.clone(), title, markdown, sort_order };
        let entry = docs_by_session.entry(session_id).or_default();
        if kind == "note" {
            entry.0 = Some(doc);
        } else {
            entry.1.push(doc);
        }
    }
    drop(rows);
    drop(stmt);

    // Action items, grouped by session_id.
    let mut actions_by_session: HashMap<String, Vec<ActionItem>> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT id, session_id, text, status, assignee_human_id, due_at, completed_at, source_order
         FROM action_items
         WHERE deleted_at IS NULL
         ORDER BY source_order",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let session_id: String = row.get(1)?;
        actions_by_session.entry(session_id).or_default().push(ActionItem {
            id: row.get(0)?,
            text: row.get(2)?,
            status: row.get(3)?,
            assignee_human_id: row.get(4)?,
            due_at: row.get(5)?,
            completed_at: row.get::<_, Option<String>>(6)?,
            source_order: row.get(7)?,
        });
    }
    drop(rows);
    drop(stmt);

    // All participants, grouped by session_id.
    let mut parts_by_session: HashMap<String, Vec<Participant>> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT id, session_id, human_id, display_name, email, role, source
         FROM session_participants
         WHERE deleted_at IS NULL",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let session_id: String = row.get(1)?;
        parts_by_session.entry(session_id.clone()).or_default().push(Participant {
            id: row.get(0)?,
            session_id,
            human_id: row.get(2)?,
            display_name: row.get(3)?,
            email: row.get(4)?,
            role: row.get(5)?,
            source: row.get(6)?,
        });
    }

    // Sessions, newest first.
    let mut stmt = conn.prepare(
        "SELECT id, title, created_at, updated_at, kind, status, started_at, ended_at, series_id
         FROM sessions WHERE deleted_at IS NULL",
    )?;
    let mut rows = stmt.query([])?;
    let mut sessions = Vec::new();
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let title: String = row.get(1)?;
        let created_at: String = row.get(2)?;
        let updated_at: String = row.get(3)?;
        let kind: String = row.get(4)?;
        let status: String = row.get(5)?;
        let started_at: String = row.get(6)?;
        let ended_at: String = row.get(7)?;
        let series_id: String = row.get(8)?;

        let mut session_transcripts = transcripts_by_session.remove(&id).unwrap_or_default();
        session_transcripts.sort_by_key(|t| t.started_at);
        let participants = parts_by_session.remove(&id).unwrap_or_default();
        let (note_doc, summaries) = docs_by_session.remove(&id).unwrap_or((None, vec![]));
        let action_items = actions_by_session.remove(&id).unwrap_or_default();
        let note = note_doc.map(|d| d.markdown).unwrap_or_default();
        let transcript = if session_transcripts.is_empty() {
            None
        } else {
            Some(TranscriptFile {
                transcripts: session_transcripts,
            })
        };

        sessions.push(Session {
            id: id.clone(),
            path: PathBuf::new(),
            meta: Meta {
                created_at,
                updated_at,
                id,
                title,
                kind,
                status,
                started_at,
                ended_at,
                series_id,
                participants,
            },
            note,
            summaries,
            action_items,
            transcript,
        });
    }

    sessions.sort_by(|a, b| b.meta.created_at.cmp(&a.meta.created_at));
    Ok(sessions)
}

/// Convert a document body to text based on its stored format.
fn body_to_text(body: &str, format: &str) -> String {
    match format {
        "prosemirror_json" => prosemirror_to_text(body),
        _ => body.trim().to_string(),
    }
}

/// Convert a ProseMirror JSON document to plain text.
/// ponytail: text-only walk; fine for embedding/search + display. Not a full
/// markdown renderer — tables/marks are flattened. Upgrade path: a real PM
/// renderer if structured markdown output is ever needed.
fn prosemirror_to_text(json: &str) -> String {
    let value: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        // Not JSON (e.g. legacy plain-text memo) — use as-is.
        Err(_) => return json.trim().to_string(),
    };
    let mut out = String::new();
    pm_walk(&value, &mut out);
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    out.trim().to_string()
}

const PM_BLOCK_TYPES: &[&str] = &[
    "paragraph",
    "heading",
    "bulletList",
    "orderedList",
    "listItem",
    "codeBlock",
    "blockquote",
];

fn pm_walk(node: &serde_json::Value, out: &mut String) {
    let ty = node.get("type").and_then(|t| t.as_str());
    match ty {
        Some("text") => {
            if let Some(t) = node.get("text").and_then(|v| v.as_str()) {
                out.push_str(t);
            }
        }
        Some("hardBreak") => out.push('\n'),
        _ => {
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    pm_walk(child, out);
                }
            }
        }
    }
    if let Some(t) = ty {
        if PM_BLOCK_TYPES.contains(&t) {
            out.push('\n');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::prosemirror_to_text;

    #[test]
    fn prosemirror_extracts_text_with_newlines() {
        let doc = r#"{"type":"doc","content":[{"type":"heading","attrs":{"level":1},"content":[{"type":"text","text":"CDL Intro Meeting"}]},{"type":"bulletList","content":[{"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"rebuild website"}]}]},{"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"automation playbook"}]}]}]}]}"#;
        let text = prosemirror_to_text(doc);
        assert!(text.contains("CDL Intro Meeting"));
        assert!(text.contains("rebuild website"));
        assert!(text.contains("automation playbook"));
        // block nodes produce newlines, so the two list items are separated
        assert!(text.contains("rebuild website\n") || text.contains("website\n\n") || text.contains("website\n"));
    }

    #[test]
    fn prosemirror_falls_back_on_plain_text() {
        // Non-JSON memo (legacy plain text) is returned as-is.
        assert_eq!(prosemirror_to_text("just plain notes"), "just plain notes");
    }

    #[test]
    fn prosemirror_empty_doc() {
        assert_eq!(prosemirror_to_text("{\"type\":\"doc\",\"content\":[]}"), "");
    }
}
