use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Session metadata from _meta.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    pub created_at: String,
    pub id: String,
    pub title: String,
    pub participants: Vec<Participant>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    pub id: String,
    pub session_id: String,
    pub source: String,
    #[serde(default)]
    pub human_id: String,
    #[serde(default)]
    pub user_id: String,
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
    pub memo: String,
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
    Memo,
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
    pub fn load(dir: &std::path::Path) -> anyhow::Result<Self> {
        let meta_path = dir.join("_meta.json");
        let memo_path = dir.join("_memo.md");
        let transcript_path = dir.join("transcript.json");

        let meta: Meta = serde_json::from_str(&std::fs::read_to_string(&meta_path)?)?;
        let memo = std::fs::read_to_string(&memo_path)?;
        let transcript = if transcript_path.exists() {
            Some(serde_json::from_str(&std::fs::read_to_string(
                &transcript_path,
            )?)?)
        } else {
            None
        };

        Ok(Self {
            id: meta.id.clone(),
            path: dir.to_path_buf(),
            meta,
            memo,
            transcript,
        })
    }

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

        // Memo as one chunk
        if !self.memo.is_empty() {
            chunks.push(Chunk {
                session_id: self.id.clone(),
                title: self.meta.title.clone(),
                chunk_type: ChunkType::Memo,
                text: self.memo.clone(),
                start_ms: None,
                end_ms: None,
                channel: None,
            });
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

/// List all session directories
pub fn session_dirs(sessions_path: &std::path::Path) -> anyhow::Result<Vec<std::path::PathBuf>> {
    let mut dirs = vec![];
    for entry in std::fs::read_dir(sessions_path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() && path.join("_meta.json").exists() {
            dirs.push(path);
        }
    }
    dirs.sort();
    Ok(dirs)
}

pub fn default_sessions_path() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("No home directory"))?;
    Ok(home.join("Library/Application Support/hyprnote/sessions"))
}
