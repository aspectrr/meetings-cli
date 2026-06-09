use crate::store::Store;
use anyhow::Result;

#[derive(serde::Serialize)]
pub struct SearchResult {
    pub rank: usize,
    pub score: f32,
    pub session_id: String,
    pub title: String,
    pub chunk_type: String,
    pub text: String,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
}

pub fn run_search(query: &str, top_k: usize, json_output: bool) -> Result<()> {
    let store = Store::load()?;

    println!("Loading embedding model...");
    let mut model = fastembed::TextEmbedding::try_new(fastembed::InitOptions::new(
        fastembed::EmbeddingModel::BGESmallENV15,
    ))?;

    let query_vecs = model.embed(vec![query.to_string()], None)?;
    let query_emb = &query_vecs[0];

    let hits = store.search(query_emb, top_k);

    let results: Vec<SearchResult> = hits
        .iter()
        .enumerate()
        .map(|(rank, (idx, score))| {
            let chunk = &store.chunks[*idx];
            SearchResult {
                rank: rank + 1,
                score: *score,
                session_id: chunk.session_id.clone(),
                title: chunk.title.clone(),
                chunk_type: match chunk.chunk_type {
                    crate::models::ChunkType::Memo => "memo".to_string(),
                    crate::models::ChunkType::TranscriptSegment => "transcript".to_string(),
                },
                text: chunk.text.clone(),
                start_ms: chunk.start_ms,
                end_ms: chunk.end_ms,
            }
        })
        .collect();

    if json_output {
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else {
        for r in &results {
            println!(
                "--- #{} (score: {:.4}) [{}] {} ---",
                r.rank, r.score, r.chunk_type, r.title
            );
            println!("{}", r.text);
            println!();
        }
    }

    Ok(())
}
