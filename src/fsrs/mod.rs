use crate::spaced_repetition::SpacedRepetiton;
use crate::utils::sort_str;
use anyhow::Result;
use chrono::DateTime;
use chrono::Duration;
use chrono::Local;
use fsrs::MemoryState;
use fsrs::DEFAULT_PARAMETERS;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

pub mod sqlite_history;

#[derive(Serialize, Deserialize, Debug)]
pub struct MemoryStateWrapper {
    pub stability: f32,
    pub difficulty: f32,
    pub interval: f32,
    pub last_reviewed: DateTime<Local>,
}

impl Default for MemoryStateWrapper {
    fn default() -> Self {
        Self {
            stability: DEFAULT_PARAMETERS[0],
            difficulty: DEFAULT_PARAMETERS[4] + 2.0 * DEFAULT_PARAMETERS[5],
            interval: 1.0,
            last_reviewed: Local::now(),
        }
    }
}

impl MemoryStateWrapper {
    pub fn next_review_time(&self) -> DateTime<Local> {
        self.last_reviewed + Duration::try_days(self.interval.round() as i64).unwrap()
    }

    fn to_memory_state(&self) -> MemoryState {
        MemoryState {
            stability: self.stability,
            difficulty: self.difficulty,
        }
    }
}

impl SpacedRepetiton for sqlite_history::SQLiteHistory {
    fn next_to_review(&self) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT word, stability, difficulty, interval, last_reviewed FROM fsrs ORDER BY RANDOM()")?;
        let person_iter = stmt.query_map([], |row| {
            let time: String = row.get(4)?;
            let sm = MemoryStateWrapper {
                stability: row.get(1)?,
                difficulty: row.get(2)?,
                interval: row.get(3)?,
                last_reviewed: DateTime::<Local>::from_str(&time).unwrap(),
            };
            let word = row.get(0)?;
            Ok((word, sm))
        })?;
        for (word, sm) in person_iter.flatten() {
            if sm.next_review_time() <= Local::now() {
                return Ok(Some(word));
            }
        }
        Ok(None)
    }

    fn add_fresh_word(&mut self, _word: String) -> Result<()> {
        unreachable!()
    }

    /// requires 1 <= q <= 4
    fn update(&self, question: String, q: u8) -> Result<()> {
        let old_state = get_word(&self.conn, &question)?;
        let next_states = self.fsrs.next_states(
            Some(old_state.to_memory_state()),
            0.9,
            (Local::now() - old_state.last_reviewed)
                .num_days()
                .abs()
                .try_into()?,
        )?;
        let new_memory_state = match q {
            1 => next_states.again,
            2 => next_states.hard,
            3 => next_states.good,
            4 => next_states.easy,
            _ => unreachable!(),
        };
        let x = MemoryStateWrapper {
            stability: new_memory_state.memory.stability,
            difficulty: new_memory_state.memory.difficulty,
            interval: new_memory_state.interval,
            last_reviewed: Local::now(),
        };
        update(&self.conn, &question, x)?;
        Ok(())
    }

    fn remove(&mut self, question: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM fsrs WHERE word = ?", [question])?;
        Ok(())
    }
}

fn update(conn: &Connection, word: &str, sm: MemoryStateWrapper) -> Result<()> {
    conn.execute(
        "UPDATE fsrs SET stability = ?2, difficulty = ?3, interval=?4, last_reviewed = ?5 WHERE word = ?1",
        (word, sm.stability, sm.difficulty, sm.interval, sm.last_reviewed.to_string()),
    )?;
    Ok(())
}

fn get_word(conn: &Connection, word: &str) -> Result<MemoryStateWrapper> {
    let sm = conn.query_row(
        "SELECT stability, difficulty, interval, last_reviewed FROM fsrs WHERE word = ?",
        [word],
        |row| {
            let time: String = row.get(3)?;
            let sm = MemoryStateWrapper {
                stability: row.get(0)?,
                difficulty: row.get(1)?,
                interval: row.get(2)?,
                last_reviewed: DateTime::<Local>::from_str(&time).unwrap(),
            };
            Ok(sm)
        },
    )?;
    Ok(sm)
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
