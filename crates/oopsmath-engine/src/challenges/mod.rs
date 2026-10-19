//! Data-driven math challenges.
//!
//! # Stage data (no schema or DAT change)
//! A challenge *is* an existing `objectives.tasks` entry of `type: math` that has
//! a `math.answer` (the schema and the Python compiler already define and
//! validate these). Its id is the task id. The prompt is a localization key:
//! `math.prompt` if present, otherwise the stage's `learning.question`.
//!
//! ```yaml
//! learning: {question: stage.001.question}
//! objectives:
//!   tasks:
//!     - id: t_math
//!       type: math
//!       math: {type: multiplication, answer: {mode: exact, value: 12}}
//! ```
//!
//! Answer rules are in [`answer`]. Math tasks with an unsupported answer mode
//! (or a generator) are skipped with a warning; malformed definitions
//! (duplicate ids, bad values, no prompt) disable the stage's challenges and are
//! reported. A stage without math tasks has no challenges.
//!
//! # Runtime
//! [`StageChallenges`] (immutable definitions) and [`ChallengeTracker`] (solved
//! flags) are rebuilt when a stage becomes active and cleared when it exits;
//! ordinary gameplay never resets them. [`submit`] is the only place a challenge
//! becomes solved, and it does so at most once. Solving changes nothing else
//! (no money, inventory, blocks or objectives other than letting an objective
//! that names the challenge complete).

pub mod answer;

use bevy::prelude::*;
use serde_json::Value;
use thiserror::Error;

use crate::app::stages::LoadedStage;
use crate::app::states::AppState;
use crate::stage::localization::{StageLocale, StageMessages};
pub use answer::{AnswerError, AnswerSpec};

#[derive(Debug, Clone, PartialEq)]
pub struct Challenge {
    /// The task id; the only way objectives refer to a challenge.
    pub id: String,
    /// Localized prompt (the key itself when untranslated).
    pub prompt: String,
    pub answer: AnswerSpec,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ChallengeError {
    #[error("challenge id '{0}' is used more than once")]
    DuplicateId(String),
    #[error("challenge '{id}': {reason}")]
    InvalidAnswer { id: String, reason: String },
    #[error("challenge '{0}' has no prompt (set math.prompt or learning.question)")]
    NoPrompt(String),
}

#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct StageChallenges {
    pub challenges: Vec<Challenge>,
    /// Why the challenges could not be used; the list is empty in that case.
    pub error: Option<String>,
}

impl StageChallenges {
    /// Parses the math tasks. The second value lists ids skipped because their
    /// answer mode is not supported.
    pub fn parse(
        objectives: &Value,
        learning: &Value,
        prompt_for: &dyn Fn(&str) -> Option<String>,
    ) -> Result<(Vec<Challenge>, Vec<String>), ChallengeError> {
        let Some(tasks) = objectives.get("tasks").and_then(Value::as_array) else {
            return Ok((Vec::new(), Vec::new()));
        };
        let mut challenges: Vec<Challenge> = Vec::new();
        let mut skipped = Vec::new();
        for task in tasks {
            let math = task.get("math");
            let answer = math.and_then(|m| m.get("answer"));
            if task.get("type").and_then(Value::as_str) != Some("math") || answer.is_none() {
                continue;
            }
            let id = task
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| ChallengeError::InvalidAnswer {
                    id: "?".into(),
                    reason: "math task without a string 'id'".into(),
                })?
                .to_string();
            if challenges.iter().any(|c| c.id == id) || skipped.contains(&id) {
                return Err(ChallengeError::DuplicateId(id));
            }
            let spec = match AnswerSpec::from_stage(answer.unwrap_or(&Value::Null)) {
                Ok(Some(spec)) => spec,
                Ok(None) => {
                    skipped.push(id);
                    continue;
                }
                Err(reason) => return Err(ChallengeError::InvalidAnswer { id, reason }),
            };
            let key = math
                .and_then(|m| m.get("prompt"))
                .and_then(Value::as_str)
                .or_else(|| learning.get("question").and_then(Value::as_str))
                .filter(|k| !k.is_empty())
                .ok_or_else(|| ChallengeError::NoPrompt(id.clone()))?;
            challenges.push(Challenge {
                prompt: prompt_for(key).unwrap_or_else(|| key.to_string()),
                id,
                answer: spec,
            });
        }
        Ok((challenges, skipped))
    }

    pub fn find(&self, id: &str) -> Option<usize> {
        self.challenges.iter().position(|c| c.id == id)
    }
}

/// Mutable per-session solved flags, index-aligned with [`StageChallenges`].
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct ChallengeTracker {
    solved: Vec<bool>,
}

impl ChallengeTracker {
    pub fn reset(&mut self, count: usize) {
        self.solved = vec![false; count];
    }

    pub fn is_solved_at(&self, index: usize) -> bool {
        self.solved.get(index).copied().unwrap_or(false)
    }

    /// Solved state by challenge id (false for unknown ids).
    pub fn is_solved(&self, challenges: &StageChallenges, id: &str) -> bool {
        challenges.find(id).is_some_and(|i| self.is_solved_at(i))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitOutcome {
    /// Transitioned from unsolved to solved (returned once per challenge).
    Solved,
    AlreadySolved,
    /// A well-formed wrong answer.
    Incorrect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SubmitError {
    #[error("This challenge is no longer available.")]
    UnknownChallenge,
    #[error(transparent)]
    Answer(#[from] AnswerError),
}

/// Validates `input` against the challenge `id`. The only way a challenge
/// becomes solved; invalid or wrong input never changes state, and a solved
/// challenge reports `AlreadySolved` instead of transitioning again.
pub fn submit(
    challenges: &StageChallenges,
    tracker: &mut ChallengeTracker,
    id: &str,
    input: &str,
) -> Result<SubmitOutcome, SubmitError> {
    let index = challenges.find(id).ok_or(SubmitError::UnknownChallenge)?;
    if index >= tracker.solved.len() {
        return Err(SubmitError::UnknownChallenge);
    }
    if tracker.solved[index] {
        return Ok(SubmitOutcome::AlreadySolved);
    }
    if challenges.challenges[index].answer.check(input)? {
        tracker.solved[index] = true;
        Ok(SubmitOutcome::Solved)
    } else {
        Ok(SubmitOutcome::Incorrect)
    }
}

pub const MAX_INPUT_LEN: usize = 24;

/// State of the challenge popup (UI-independent so it can be tested).
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct ChallengePanel {
    pub open: bool,
    /// Index into [`StageChallenges::challenges`].
    pub index: usize,
    pub input: String,
    pub feedback: Option<String>,
}

impl ChallengePanel {
    pub fn close(&mut self) {
        *self = Self::default();
    }

    /// Opens on the first unsolved challenge (or the first one).
    pub fn open_for(&mut self, challenges: &StageChallenges, tracker: &ChallengeTracker) {
        if challenges.challenges.is_empty() {
            return;
        }
        *self = Self {
            open: true,
            index: (0..challenges.challenges.len())
                .find(|i| !tracker.is_solved_at(*i))
                .unwrap_or(0),
            ..Self::default()
        };
    }

    /// Moves to another challenge, wrapping, and clears the input.
    pub fn step(&mut self, delta: i32, count: usize) {
        if count == 0 {
            return;
        }
        self.index = (self.index as i32 + delta).rem_euclid(count as i32) as usize;
        self.input.clear();
        self.feedback = None;
    }

    /// Accepts only characters a numeric answer can contain.
    pub fn push_char(&mut self, c: char) {
        if (c.is_ascii_digit() || matches!(c, '.' | '-' | '+'))
            && self.input.chars().count() < MAX_INPUT_LEN
        {
            self.input.push(c);
            self.feedback = None;
        }
    }

    pub fn backspace(&mut self) {
        self.input.pop();
    }

    /// Submits the current input for the selected challenge, setting feedback.
    /// Empty input is refused without touching the tracker.
    pub fn submit(&mut self, challenges: &StageChallenges, tracker: &mut ChallengeTracker) {
        let Some(challenge) = challenges.challenges.get(self.index) else {
            self.feedback = Some(SubmitError::UnknownChallenge.to_string());
            return;
        };
        if self.input.trim().is_empty() {
            self.feedback = Some(AnswerError::Empty.to_string());
            return;
        }
        self.feedback = Some(
            match submit(challenges, tracker, &challenge.id, &self.input) {
                Ok(SubmitOutcome::Solved) => {
                    self.input.clear();
                    "Correct! Challenge solved.".to_string()
                }
                Ok(SubmitOutcome::AlreadySolved) => "Already solved.".to_string(),
                Ok(SubmitOutcome::Incorrect) => "Not correct. Try again.".to_string(),
                Err(err) => err.to_string(),
            },
        );
    }
}

pub struct ChallengesPlugin;

impl Plugin for ChallengesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StageChallenges>()
            .init_resource::<ChallengeTracker>()
            .init_resource::<ChallengePanel>()
            .add_systems(OnEnter(AppState::InGame), init_challenges)
            .add_systems(OnExit(AppState::InGame), clear_challenges);
    }
}

/// Builds the challenges for the stage that just became active.
pub fn init_challenges(
    loaded: Res<LoadedStage>,
    locale: Res<StageLocale>,
    mut challenges: ResMut<StageChallenges>,
    mut tracker: ResMut<ChallengeTracker>,
    mut panel: ResMut<ChallengePanel>,
) {
    panel.close();
    let Some(package) = loaded.0.as_ref() else {
        *challenges = StageChallenges::default();
        tracker.reset(0);
        return;
    };
    let messages = StageMessages::from_localizations(&package.localization);
    let prompt_for = |key: &str| {
        messages
            .resolve(key, &locale.locale, &locale.fallback)
            .map(str::to_owned)
    };
    *challenges = match StageChallenges::parse(
        &package.stage.objectives,
        &package.stage.learning,
        &prompt_for,
    ) {
        Ok((list, skipped)) => {
            for id in skipped {
                warn!("Math task '{id}' uses an unsupported answer mode and is not a challenge");
            }
            StageChallenges {
                challenges: list,
                error: None,
            }
        }
        Err(err) => {
            error!(
                "Invalid math challenges in stage '{}': {err}",
                package.meta.stage_id
            );
            StageChallenges {
                challenges: Vec::new(),
                error: Some(err.to_string()),
            }
        }
    };
    tracker.reset(challenges.challenges.len());
}

fn clear_challenges(
    mut challenges: ResMut<StageChallenges>,
    mut tracker: ResMut<ChallengeTracker>,
    mut panel: ResMut<ChallengePanel>,
) {
    *challenges = StageChallenges::default();
    tracker.reset(0);
    panel.close();
}

#[cfg(test)]
mod tests;
