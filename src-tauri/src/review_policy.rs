use crate::error::AppError;
use serde::{Deserialize, Serialize};

pub const POLICY_VERSION: &str = "svl-review-1";
const MINUTE: i64 = 60_000;
const DAY: i64 = 24 * 60 * MINUTE;
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Correct,
    CorrectWithHint,
    Incorrect,
    NeedsConfirmation,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ReviewState {
    pub level: String,
    pub streak: u32,
    pub lapses: u32,
    pub interval_ms: i64,
    pub due_at: i64,
    pub last_reviewed_at: i64,
    pub recent: Vec<Outcome>,
    pub leech_active: bool,
    pub leech_started_at: i64,
    pub leech_cleared_at: i64,
}
impl Default for ReviewState {
    fn default() -> Self {
        Self {
            level: "new".into(),
            streak: 0,
            lapses: 0,
            interval_ms: 0,
            due_at: 0,
            last_reviewed_at: 0,
            recent: Vec::new(),
            leech_active: false,
            leech_started_at: 0,
            leech_cleared_at: 0,
        }
    }
}
impl ReviewState {
    pub fn is_leech(&self, collections_since_clear: u32) -> bool {
        self.leech_active || (collections_since_clear >= 3 && self.streak < 3)
    }
    pub fn apply(
        &self,
        outcome: Outcome,
        now: i64,
        relearn_at: i64,
        collections_since_clear: u32,
    ) -> Result<Self, AppError> {
        if outcome == Outcome::NeedsConfirmation {
            return Err(AppError::new(
                "invalid_input",
                "待确认结果不能用于复习调度。",
            ));
        }
        if now < self.last_reviewed_at || now < 0 {
            return Err(AppError::new("invalid_input", "测验时间顺序无效。"));
        }
        let mut next = self.clone();
        if relearn_at > self.last_reviewed_at && self.last_reviewed_at > 0 {
            next.level = "learning".into();
            next.streak = 0;
            next.interval_ms = 0;
            next.due_at = now;
        }
        let eligible = next.last_reviewed_at == 0 || now >= next.due_at;
        if next.is_leech(collections_since_clear) && !next.leech_active {
            next.leech_active = true;
            next.leech_started_at = now;
        }
        next.recent.push(outcome);
        if next.recent.len() > 5 {
            next.recent.remove(0);
        }
        match outcome {
            Outcome::Correct if eligible => {
                next.streak = next.streak.saturating_add(1);
                let intervals = [1, 3, 7, 14, 30, 60, 120, 180];
                let days =
                    intervals[(next.streak.saturating_sub(1) as usize).min(intervals.len() - 1)];
                next.interval_ms = days * DAY;
                next.due_at = now
                    .checked_add(next.interval_ms)
                    .ok_or_else(|| AppError::new("invalid_input", "复习日期超出范围。"))?;
                next.level = if next.streak >= 5 {
                    "mastered"
                } else {
                    "review"
                }
                .into();
                if next.streak >= 3 && next.leech_active {
                    next.leech_active = false;
                    next.leech_cleared_at = now;
                    next.recent.clear();
                }
            }
            Outcome::Correct => {}
            Outcome::CorrectWithHint | Outcome::Incorrect => {
                next.streak = 0;
                next.level = "learning".into();
                next.interval_ms = if outcome == Outcome::Incorrect {
                    5 * MINUTE
                } else {
                    10 * MINUTE
                };
                next.due_at = now
                    .checked_add(next.interval_ms)
                    .ok_or_else(|| AppError::new("invalid_input", "复习日期超出范围。"))?;
                if outcome == Outcome::Incorrect {
                    next.lapses = next.lapses.saturating_add(1);
                }
            }
            Outcome::NeedsConfirmation => unreachable!(),
        }
        if next
            .recent
            .iter()
            .filter(|result| **result == Outcome::Incorrect)
            .count()
            >= 3
            && !next.leech_active
        {
            next.leech_active = true;
            next.leech_started_at = now;
        }
        next.last_reviewed_at = now;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn correct_hint_and_wrong_have_distinct_schedules() {
        let initial = ReviewState::default();
        let now = 1_000;
        assert_eq!(
            initial.apply(Outcome::Correct, now, 0, 1).unwrap().due_at,
            now + DAY
        );
        assert_eq!(
            initial
                .apply(Outcome::CorrectWithHint, now, 0, 1)
                .unwrap()
                .due_at,
            now + 10 * MINUTE
        );
        assert_eq!(
            initial.apply(Outcome::Incorrect, now, 0, 1).unwrap().due_at,
            now + 5 * MINUTE
        );
    }
    #[test]
    fn rapid_correct_answers_cannot_create_mastery() {
        let mut state = ReviewState::default();
        for time in 1..8 {
            state = state.apply(Outcome::Correct, time, 0, 1).unwrap();
        }
        assert_eq!(state.streak, 1);
        assert_ne!(state.level, "mastered");
        assert_eq!(state.due_at, 1 + DAY);
    }
    #[test]
    fn spaced_passes_reach_mastery_and_recollection_restarts_consolidation() {
        let mut state = ReviewState::default();
        for _ in 0..5 {
            let now = state.due_at.max(1);
            state = state.apply(Outcome::Correct, now, 0, 1).unwrap();
        }
        assert_eq!(state.level, "mastered");
        let repeated = state
            .apply(
                Outcome::Correct,
                state.last_reviewed_at + 10,
                state.last_reviewed_at + 5,
                1,
            )
            .unwrap();
        assert_eq!(repeated.streak, 1);
        assert_eq!(repeated.interval_ms, DAY);
        assert_eq!(repeated.level, "review");
    }
    #[test]
    fn leech_requires_spaced_improvement_to_exit() {
        let mut state = ReviewState::default();
        for time in 1..=3 {
            state = state.apply(Outcome::Incorrect, time, 0, 1).unwrap();
        }
        assert!(state.is_leech(1));
        for time in 4..7 {
            state = state.apply(Outcome::Correct, time, 0, 1).unwrap();
        }
        assert!(state.is_leech(1));
        for _ in 0..3 {
            state = state.apply(Outcome::Correct, state.due_at, 0, 1).unwrap();
        }
        assert!(!state.leech_active);
        assert!(state.leech_cleared_at > 0);
        assert!(!state.is_leech(0));
        for _ in 0..3 {
            state = state.apply(Outcome::Incorrect, state.due_at, 0, 0).unwrap();
        }
        assert!(state.leech_active);
    }
    #[test]
    fn repeated_collection_enters_special_without_a_test_failure() {
        let state = ReviewState::default();
        assert!(!state.is_leech(2));
        assert!(state.is_leech(3));
        let state = state.apply(Outcome::Correct, 1, 0, 3).unwrap();
        assert!(state.leech_active);
        assert_eq!(state.lapses, 0);
    }
}
