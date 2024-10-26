use anyhow::Result;

pub trait SpacedRepetiton: Sized + Default {
    /// find next reviewable word
    fn next_to_review(&self) -> Result<Option<String>>;

    fn update(&self, question: String, q: u8) -> Result<()>;

    fn remove(&mut self, question: &str) -> Result<()>;
}
