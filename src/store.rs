use crate::models::Chunk;
use anyhow::Result;
use std::path::PathBuf;

pub const STORE_DIR: &str = ".meetings-cli";

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Store {
    pub chunks: Vec<Chunk>,
    pub embeddings: Vec<Vec<f32>>,
}

impl Store {
    pub fn default_path() -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("No home directory"))?;
        Ok(home.join(STORE_DIR).join("index.bin"))
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::default_path()?;
        let data = bincode::serialize(self)?;
        std::fs::write(&path, data)?;
        Ok(())
    }

    pub fn load() -> Result<Self> {
        let path = Self::default_path()?;
        let data = std::fs::read(&path)?;
        Ok(bincode::deserialize(&data)?)
    }

    /// Cosine similarity
    fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }
        dot / (norm_a * norm_b)
    }

    /// Search for top-k chunks matching query embedding
    pub fn search(&self, query_embedding: &[f32], top_k: usize) -> Vec<(usize, f32)> {
        let mut scored: Vec<(usize, f32)> = self
            .embeddings
            .iter()
            .enumerate()
            .map(|(i, emb)| (i, Self::cosine_sim(query_embedding, emb)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);
        scored
    }
}
