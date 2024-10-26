use anyhow::Result;

pub trait SpacedRepetiton: Sized {
    /// find next reviewable word
    async fn next_to_review(&self) -> Result<Option<String>>;

    async fn update(&self, question: &str, rating: u8) -> Result<()>;

    async fn remove(&mut self, question: &str) -> Result<()>;
}
