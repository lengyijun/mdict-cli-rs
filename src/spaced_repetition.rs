use anyhow::Result;
use rs_fsrs::Rating;

pub trait SpacedRepetiton: Sized {
    /// find next reviewable word
    async fn next_to_review(&self) -> Result<String>;

    async fn update(&self, question: &str, rating: Rating) -> Result<()>;

    async fn remove(&mut self, question: &str) -> Result<()>;
}
