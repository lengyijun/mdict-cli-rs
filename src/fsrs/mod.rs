use crate::spaced_repetition::SpacedRepetiton;
use anyhow::Result;
use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use fsrs::Card;
use sqlx::Row;
use sqlx::SqlitePool;

pub mod sqlite_history;

impl SpacedRepetiton for sqlite_history::SQLiteHistory {
    async fn next_to_review(&self) -> Result<Option<String>> {
        let stmt: Vec<(String, String)> =
            sqlx::query_as("SELECT word, card FROM fsrs ORDER BY RANDOM()")
                .fetch_all(&self.conn)
                .await?;
        for row in stmt {
            let card_str = row.1;
            let word = row.0;
            let card: Card = serde_json::from_str(&card_str)?;
            if card.due <= Utc::now() {
                return Ok(Some(word.to_owned()));
            }
        }
        Ok(None)
    }

    /// requires 1 <= rating <= 4
    async fn update(&self, question: String, rating: u8) -> Result<()> {
        let rating = match rating {
            1 => fsrs::Rating::Again,
            2 => fsrs::Rating::Hard,
            3 => fsrs::Rating::Good,
            4 => fsrs::Rating::Easy,
            _ => unreachable!(),
        };
        let old_card = get_word(&self.conn, &question).await?;
        let scheduling_info = self.fsrs.next(old_card, Utc::now(), rating);
        update(&self.conn, &question, scheduling_info.card).await?;
        Ok(())
    }

    async fn remove(&mut self, question: &str) -> Result<()> {
        sqlx::query("DELETE FROM fsrs WHERE word = $1")
            .bind(question)
            .fetch_one(&self.conn)
            .await?;
        Ok(())
    }
}

// TODO: never used ?
async fn update(pool: &SqlitePool, word: &str, card: Card) -> Result<()> {
    let card_str: String = serde_json::to_string(&card)?;
    sqlx::query("UPDATE fsrs SET card = $2 WHERE word = $1")
        .bind(word)
        .bind(card_str)
        .fetch_one(pool)
        .await?;
    Ok(())
}

async fn get_word(pool: &SqlitePool, word: &str) -> Result<Card> {
    let card_str: String = sqlx::query("SELECT card FROM fsrs WHERE word = $1")
        .bind(word)
        .fetch_one(pool)
        .await?
        .get::<String, _>(0);
    let card: Card = serde_json::from_str(&card_str)?;
    Ok(card)
}
