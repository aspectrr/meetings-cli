use crate::models::{default_sessions_path, session_dirs, Chunk, Session};
use crate::store::Store;
use anyhow::Result;

/// Index all sessions: chunk text, embed, save to store
pub fn run_index(sessions_path: Option<&std::path::Path>, segment_ms: i64) -> Result<()> {
    let sp = match sessions_path {
        Some(p) => p.to_path_buf(),
        None => default_sessions_path()?,
    };

    println!("Loading sessions from {}...", sp.display());
    let dirs = session_dirs(&sp)?;
    let mut sessions = Vec::new();
    for d in &dirs {
        match Session::load(d) {
            Ok(s) => {
                println!("  Loaded: {} ({})", s.meta.title, s.id);
                sessions.push(s);
            }
            Err(e) => eprintln!("  Skipping {}: {e}", d.display()),
        }
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
