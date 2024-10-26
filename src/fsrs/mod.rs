use crate::spaced_repetition::SpacedRepetiton;
use crate::utils::sort_str;
use anyhow::Result;
use chrono::DateTime;
use chrono::Duration;
use chrono::Local;
use chrono::Utc;
use fsrs::Card;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

pub mod sqlite_history;

fn next_review_time(last_reviewed: DateTime<Utc>, interval: i64) -> DateTime<Utc> {
    last_reviewed + Duration::try_days(interval).unwrap()
}

impl SpacedRepetiton for sqlite_history::SQLiteHistory {
    fn next_to_review(&self) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT word, card FROM fsrs ORDER BY RANDOM()")?;
        let person_iter = stmt.query_map([], |row| {
            let card_str: String = row.get(1)?;
            let word = row.get(0)?;
            Ok((word, card_str))
        })?;
        for (word, card_str) in person_iter.flatten() {
            let card: Card = serde_json::from_str(&card_str)?;
            if next_review_time(card.last_review, card.scheduled_days) <= Utc::now() {
                return Ok(Some(word));
            }
        }
        Ok(None)
    }

    /// requires 1 <= q <= 4
    fn update(&self, question: String, rating: u8) -> Result<()> {
        let rating = match rating {
            1 => fsrs::Rating::Again,
            2 => fsrs::Rating::Hard,
            3 => fsrs::Rating::Good,
            4 => fsrs::Rating::Easy,
            _ => unreachable!(),
        };
        let old_card = get_word(&self.conn, &question)?;
        let scheduling_info = self.fsrs.next(old_card, Utc::now(), rating);
        update(&self.conn, &question, scheduling_info.card)?;
        Ok(())
    }

    fn remove(&mut self, question: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM fsrs WHERE word = ?", [question])?;
        Ok(())
    }
}

fn update(conn: &Connection, word: &str, card: Card) -> Result<()> {
    let card_str: String = serde_json::to_string(&card)?;
    conn.execute(
        "UPDATE fsrs SET card = ?2 5 WHERE word = ?1",
        (word, card_str),
    )?;
    Ok(())
}

fn get_word(conn: &Connection, word: &str) -> Result<Card> {
    let card_str = conn.query_row("SELECT card FROM fsrs WHERE word = ?", [word], |row| {
        let card_str: String = row.get(0)?;
        Ok(card_str)
    })?;
    let card: Card = serde_json::from_str(&card_str)?;
    Ok(card)
}

impl sqlite_history::SQLiteHistory {
    pub fn fuzzy_lookup_in_history(&self, target_word: &str, threhold: usize) -> Vec<String> {
        let sorted_targetword = sort_str(target_word);
        let mut stmt = self.conn.prepare("SELECT word FROM fsrs").unwrap();
        stmt.query_map([], |row| {
            let word: String = row.get(0).unwrap();
            if strsim::levenshtein(&word, target_word) <= threhold
                || sort_str(&word) == sorted_targetword
            {
                Ok(word)
            } else {
                Err(rusqlite::Error::ExecuteReturnedResults)
            }
        })
        .unwrap()
        .flatten()
        .collect()
    }
}
