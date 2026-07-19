use crate::models::{load_sessions, Chunk};
use crate::store::Store;
use anyhow::Result;

/// Index all sessions: chunk text, embed, save to store
pub fn run_index(db_path: &std::path::Path, segment_ms: i64) -> Result<()> {
    println!("Loading sessions from {}...", db_path.display());
    let sessions = load_sessions(db_path)?;
    for s in &sessions {
        println!("  Loaded: {} ({})", s.meta.title, s.id);
    }

    println!("Building chunks (segment duration: {segment_ms}ms)...");
    let all_chunks: Vec<Chunk> = sessions.iter().flat_map(|s| s.chunks(segment_ms)).collect();
    println!("  {} chunks total", all_chunks.len());

    let texts: Vec<String> = all_chunks.iter().map(|c| c.text.clone()).collect();

    println!("Loading embedding model...");
    let mut model = fastembed::TextEmbedding::try_new(
        fastembed::InitOptions::new(fastembed::EmbeddingModel::BGESmallENV15)
            .with_show_download_progress(true),
    )?;

    println!("Embedding {} texts...", texts.len());
    let embeddings = model.embed(texts, None)?;

    let store = Store {
        chunks: all_chunks,
        embeddings,
    };

    let store_path = Store::default_path()?;
    println!("Saving index to {}...", store_path.display());
    std::fs::create_dir_all(store_path.parent().unwrap())?;
    store.save()?;

    println!("Done. Indexed {} chunks.", store.chunks.len());
    Ok(())
}
