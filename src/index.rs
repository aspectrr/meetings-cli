use crate::models::{load_sessions, Chunk};
use crate::store::Store;
use anyhow::Result;
use std::collections::HashSet;

/// Core indexing logic — no printing. Returns the built store.
/// Used by both CLI and MCP server.
pub fn build_index(db_path: &std::path::Path, segment_ms: i64) -> Result<Store> {
    let sessions = load_sessions(db_path)?;

    let all_chunks: Vec<Chunk> = sessions.iter().flat_map(|s| s.chunks(segment_ms)).collect();

    let texts: Vec<String> = all_chunks.iter().map(|c| c.text.clone()).collect();

    let mut model = fastembed::TextEmbedding::try_new(
        fastembed::InitOptions::new(fastembed::EmbeddingModel::BGESmallENV15)
            .with_show_download_progress(false),
    )?;

    let embeddings = model.embed(texts, None)?;

    Ok(Store {
        chunks: all_chunks,
        embeddings,
    })
}

/// Check whether the stored index is stale (session count changed).
pub fn index_is_stale(db_path: &std::path::Path) -> Result<bool> {
    let store = match Store::load() {
        Ok(s) => s,
        Err(_) => return Ok(true),
    };

    let sessions = load_sessions(db_path)?;
    let indexed_ids: HashSet<&str> = store.chunks.iter().map(|c| c.session_id.as_str()).collect();

    Ok(indexed_ids.len() != sessions.len())
}

/// Ensure index is fresh, rebuilding if needed. Saves to disk.
pub fn ensure_fresh_index(db_path: &std::path::Path) -> Result<()> {
    if index_is_stale(db_path)? {
        let store = build_index(db_path, 60000)?;
        let store_path = Store::default_path()?;
        if let Some(parent) = store_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        store.save()?;
    }
    Ok(())
}

/// CLI wrapper — prints progress, builds, saves.
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
