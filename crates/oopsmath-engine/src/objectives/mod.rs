//! Data-driven construction objectives.
//!
//! # Where objectives come from (no schema or DAT change)
//! The existing `objectives.tasks` list already carries typed tasks. A
//! construction task whose free-form `data` object has a `kind` is read here:
//!
//! ```yaml
//! objectives:
//!   tasks:
//!     - id: t_logs
//!       type: construction
//!       data: {kind: place_block, block: block.wood_log, count: 12, title: stage.001.logs}
//! ```
//!
//! `kind` is the extension point (only `place_block` exists). `title` is an
//! optional localization key; without a translation the block's display name is
//! used. Tasks without `data.kind` (math tasks, the region-target tasks of
//! stage 001) are not this system's business and are ignored, so legacy stages
//! load unchanged and an empty list means "no tracked objectives".
//!
//! # Evaluation
//! Progress is derived from [`PlacedWorld`], the authoritative record of
//! player-placed blocks: the number of placed *instances* of the block (a
//! rotated multi-cell block is one). Previews, inventory, purchases and
//! stage-authored content are never counted (the runtime places no
//! stage-authored blocks in `PlacedWorld`). Evaluation runs only when
//! `PlacedWorld` or the objective list changed.
//!
//! # Completion policy
//! Before completion, progress follows the live count (it drops if a counted
//! block is removed). Completion is **latched** for the stage session: once an
//! objective completes it stays complete, shown as `count / count`, and is
//! reported exactly once. The aggregate state needs at least one objective, so
//! an empty list never reads as "mission complete". Rewards, unlocks and stage
//! transitions are not implemented.

use bevy::prelude::*;
use serde_json::Value;
use thiserror::Error;

use crate::app::stages::LoadedStage;
use crate::app::states::AppState;
use crate::blocks::stage_rules::StagePlacementRules;
use crate::blocks::{BlockId, BlockRegistry, PlacedWorld};
use crate::challenges::{ChallengeTracker, StageChallenges, init_challenges};
use crate::stage::localization::{StageLocale, StageMessages};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectiveKind {
    /// Have `count` instances of `block` placed at the same time.
    PlaceBlock { block: BlockId, count: u32 },
}

/// Immutable definition (from stage data). Runtime progress lives in
/// [`ObjectiveTracker`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Objective {
    pub id: String,
    /// Localized title, or the block's display name.
    pub title: String,
    pub kind: ObjectiveKind,
    /// Optional prerequisite: id of a math challenge that must also be solved.
    pub requires_challenge: Option<String>,
}

impl Objective {
    pub fn required(&self) -> u32 {
        match &self.kind {
            ObjectiveKind::PlaceBlock { count, .. } => *count,
        }
    }

    /// Live progress from the placed blocks, never above the requirement.
    pub fn live_progress(&self, world: &PlacedWorld) -> u32 {
        match &self.kind {
            ObjectiveKind::PlaceBlock { block, count } => world.count_of(block).min(*count),
        }
    }

    /// `Wood Log: 7 / 12`, with a marker once complete.
    pub fn label(&self, state: &ObjectiveState) -> String {
        let mark = if state.completed {
            "  (done)"
        } else if state.awaiting_challenge {
            "  - solve the math challenge to finish"
        } else {
            ""
        };
        format!(
            "{}: {} / {}{mark}",
            self.title,
            state.progress,
            self.required()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ObjectiveError {
    #[error("objective id '{0}' is used more than once")]
    DuplicateId(String),
    #[error("objective '{id}': unsupported kind '{kind}' (expected 'place_block')")]
    UnknownKind { id: String, kind: String },
    #[error("objective '{id}': {reason}")]
    InvalidField { id: String, reason: String },
    #[error("objective '{id}': block '{block}' is not registered")]
    UnknownBlock { id: String, block: String },
    #[error("objective '{id}': block '{block}' is not allowed in this stage")]
    NotAllowed { id: String, block: String },
    #[error("objective '{id}': challenge '{challenge}' does not exist")]
    UnknownChallenge { id: String, challenge: String },
}

/// The active stage's objectives (immutable once built).
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct StageObjectives {
    pub objectives: Vec<Objective>,
    /// Why the objectives could not be used; the list is empty in that case.
    pub error: Option<String>,
}

impl StageObjectives {
    /// Parses and validates `objectives.tasks`. `title_for` resolves a
    /// localization key (`None` when untranslated).
    pub fn parse(
        objectives: &Value,
        registry: &BlockRegistry,
        rules: &StagePlacementRules,
        title_for: &dyn Fn(&str) -> Option<String>,
    ) -> Result<Vec<Objective>, ObjectiveError> {
        let Some(tasks) = objectives.get("tasks").and_then(Value::as_array) else {
            return Ok(Vec::new());
        };
        let mut parsed: Vec<Objective> = Vec::new();
        for task in tasks {
            let data = task.get("data");
            let is_block_count = task.get("type").and_then(Value::as_str) == Some("construction")
                && data.and_then(|d| d.get("kind")).is_some();
            if !is_block_count {
                continue;
            }
            let data = data.unwrap_or(&Value::Null);
            let id = task
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| ObjectiveError::InvalidField {
                    id: "?".into(),
                    reason: "task without a string 'id'".into(),
                })?
                .to_string();
            if parsed.iter().any(|o| o.id == id) {
                return Err(ObjectiveError::DuplicateId(id));
            }
            let kind = data.get("kind").and_then(Value::as_str).unwrap_or("");
            if kind != "place_block" {
                return Err(ObjectiveError::UnknownKind {
                    id,
                    kind: kind.to_string(),
                });
            }
            let invalid = |reason: &str| ObjectiveError::InvalidField {
                id: id.clone(),
                reason: reason.to_string(),
            };
            let block = data
                .get("block")
                .and_then(Value::as_str)
                .filter(|b| !b.is_empty())
                .ok_or_else(|| invalid("'data.block' must be a block id string"))?;
            let count = data
                .get("count")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| *n >= 1)
                .ok_or_else(|| invalid("'data.count' must be an integer of at least 1"))?;
            let block_id = BlockId::new(block);
            let def = registry
                .get(&block_id)
                .ok_or_else(|| ObjectiveError::UnknownBlock {
                    id: id.clone(),
                    block: block.to_string(),
                })?;
            if !rules.is_allowed(&block_id) {
                return Err(ObjectiveError::NotAllowed {
                    id,
                    block: block.to_string(),
                });
            }
            let requires_challenge = match data.get("requires_challenge") {
                None | Some(Value::Null) => None,
                Some(value) => Some(
                    value
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .ok_or_else(|| {
                            invalid("'data.requires_challenge' must be a challenge id string")
                        })?
                        .to_string(),
                ),
            };
            let title = data
                .get("title")
                .and_then(Value::as_str)
                .and_then(title_for)
                .unwrap_or_else(|| def.display_name.clone());
            parsed.push(Objective {
                id,
                title,
                kind: ObjectiveKind::PlaceBlock {
                    block: block_id,
                    count,
                },
                requires_challenge,
            });
        }
        Ok(parsed)
    }

    /// Rejects objectives whose `requires_challenge` names a challenge that
    /// does not exist (an unresolved reference is never silently ignored).
    /// Challenges never depend on objectives, so no cycle is possible.
    pub fn with_checked_challenge_refs(self, exists: &dyn Fn(&str) -> bool) -> Self {
        let missing = self.objectives.iter().find_map(|o| {
            let challenge = o.requires_challenge.as_deref()?;
            (!exists(challenge)).then(|| ObjectiveError::UnknownChallenge {
                id: o.id.clone(),
                challenge: challenge.to_string(),
            })
        });
        match missing {
            Some(err) => Self {
                objectives: Vec::new(),
                error: Some(err.to_string()),
            },
            None => self,
        }
    }

    /// Like [`parse`](Self::parse), but a malformed list becomes an empty list
    /// plus a diagnostic instead of an error.
    pub fn from_stage(
        objectives: &Value,
        registry: &BlockRegistry,
        rules: &StagePlacementRules,
        title_for: &dyn Fn(&str) -> Option<String>,
    ) -> Self {
        match Self::parse(objectives, registry, rules, title_for) {
            Ok(objectives) => Self {
                objectives,
                error: None,
            },
            Err(err) => Self {
                objectives: Vec::new(),
                error: Some(err.to_string()),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ObjectiveState {
    pub progress: u32,
    pub completed: bool,
    /// The construction condition is met but the required challenge is not
    /// solved yet.
    pub awaiting_challenge: bool,
}

/// Mutable per-session progress, index-aligned with [`StageObjectives`].
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct ObjectiveTracker {
    states: Vec<ObjectiveState>,
}

impl ObjectiveTracker {
    pub fn reset(&mut self, objective_count: usize) {
        self.states = vec![ObjectiveState::default(); objective_count];
    }

    pub fn states(&self) -> &[ObjectiveState] {
        &self.states
    }

    /// True only when there is at least one objective and all are complete.
    pub fn all_complete(&self) -> bool {
        !self.states.is_empty() && self.states.iter().all(|s| s.completed)
    }
}

/// Recomputes progress from the placed blocks and returns the indices of
/// objectives that completed *now*. Completed objectives are latched, so each
/// index is returned at most once per session.
pub fn evaluate(
    objectives: &[Objective],
    world: &PlacedWorld,
    tracker: &mut ObjectiveTracker,
) -> Vec<usize> {
    // Without a challenge source, a prerequisite counts as unsolved.
    evaluate_with(objectives, world, &|_| false, tracker)
}

/// [`evaluate`] with a lookup for solved challenges. An objective completes
/// only when its construction condition *and* its challenge (if any) are both
/// satisfied; neither can complete the other.
pub fn evaluate_with(
    objectives: &[Objective],
    world: &PlacedWorld,
    challenge_solved: &dyn Fn(&str) -> bool,
    tracker: &mut ObjectiveTracker,
) -> Vec<usize> {
    if tracker.states.len() != objectives.len() {
        tracker.reset(objectives.len());
    }
    let mut newly_completed = Vec::new();
    for (i, objective) in objectives.iter().enumerate() {
        let state = &mut tracker.states[i];
        if state.completed {
            continue;
        }
        state.progress = objective.live_progress(world);
        let built = state.progress >= objective.required();
        let solved = objective
            .requires_challenge
            .as_deref()
            .is_none_or(challenge_solved);
        state.awaiting_challenge = built && !solved;
        if built && solved {
            state.completed = true;
            newly_completed.push(i);
        }
    }
    newly_completed
}

pub struct ObjectivesPlugin;

impl Plugin for ObjectivesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StageObjectives>()
            .init_resource::<ObjectiveTracker>()
            .add_systems(
                OnEnter(AppState::InGame),
                init_objectives.after(init_challenges),
            )
            .add_systems(OnExit(AppState::InGame), clear_objectives)
            .add_systems(
                Update,
                evaluate_objectives.run_if(in_state(AppState::InGame)),
            );
    }
}

/// Builds the objectives from the loaded stage. The placement rules are
/// derived from the same stage data, so no system ordering is needed.
fn init_objectives(
    loaded: Res<LoadedStage>,
    registry: Res<BlockRegistry>,
    locale: Res<StageLocale>,
    challenges: Res<StageChallenges>,
    mut objectives: ResMut<StageObjectives>,
    mut tracker: ResMut<ObjectiveTracker>,
) {
    let Some(package) = loaded.0.as_ref() else {
        *objectives = StageObjectives::default();
        tracker.reset(0);
        return;
    };
    let rules = StagePlacementRules::from_construction(&package.stage.construction);
    let messages = StageMessages::from_localizations(&package.localization);
    let title_for = |key: &str| {
        messages
            .resolve(key, &locale.locale, &locale.fallback)
            .map(str::to_owned)
    };
    *objectives =
        StageObjectives::from_stage(&package.stage.objectives, &registry, &rules, &title_for)
            .with_checked_challenge_refs(&|id| challenges.find(id).is_some());
    if let Some(err) = &objectives.error {
        error!(
            "Invalid objectives in stage '{}': {err}",
            package.meta.stage_id
        );
    }
    tracker.reset(objectives.objectives.len());
}

fn clear_objectives(
    mut objectives: ResMut<StageObjectives>,
    mut tracker: ResMut<ObjectiveTracker>,
) {
    *objectives = StageObjectives::default();
    tracker.reset(0);
}

/// Re-evaluates only when the placed blocks or the objective list changed.
fn evaluate_objectives(
    world: Res<PlacedWorld>,
    objectives: Res<StageObjectives>,
    challenges: Res<StageChallenges>,
    solved: Res<ChallengeTracker>,
    mut tracker: ResMut<ObjectiveTracker>,
) {
    if !(world.is_changed()
        || objectives.is_changed()
        || solved.is_changed()
        || challenges.is_changed())
    {
        return;
    }
    let completed = evaluate_with(
        &objectives.objectives,
        &world,
        &|id| solved.is_solved(&challenges, id),
        &mut tracker,
    );
    for &i in &completed {
        info!("Objective '{}' completed", objectives.objectives[i].id);
    }
    if !completed.is_empty() && tracker.all_complete() {
        info!("All stage objectives completed");
    }
}

#[cfg(test)]
mod tests;
