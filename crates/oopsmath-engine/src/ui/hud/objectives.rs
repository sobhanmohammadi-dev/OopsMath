//! Objectives section of the gameplay HUD (inside the top-left stage panel).
//!
//! Presentation only: rows come from [`StageObjectives`] (definitions) and
//! [`ObjectiveTracker`] (progress). Rows are plain text nodes without an
//! `Interaction`, so they never intercept clicks. Rebuilt only when objectives
//! or progress change.

use bevy::prelude::*;

use crate::objectives::{ObjectiveTracker, StageObjectives};
use crate::ui::components::{HudObjectiveList, HudObjectiveRow};
use crate::ui::theme::UiTheme;

pub fn refresh_objectives(
    mut commands: Commands,
    objectives: Res<StageObjectives>,
    tracker: Res<ObjectiveTracker>,
    theme: Res<UiTheme>,
    list: Query<Entity, With<HudObjectiveList>>,
    rows: Query<Entity, With<HudObjectiveRow>>,
) {
    if !(objectives.is_changed() || tracker.is_changed()) {
        return;
    }
    for row in &rows {
        commands.entity(row).despawn();
    }
    let Ok(container) = list.single() else {
        return;
    };
    let font = |size: f32| TextFont {
        font_size: FontSize::Px(size),
        ..default()
    };
    commands.entity(container).with_children(|parent| {
        if let Some(error) = &objectives.error {
            parent.spawn((
                HudObjectiveRow,
                Text::new(format!("Objectives are invalid: {error}")),
                font(15.0),
                TextColor(theme.secondary_text),
            ));
            return;
        }
        // No objectives: nothing is shown (and no completion message).
        if objectives.objectives.is_empty() {
            return;
        }
        parent.spawn((
            HudObjectiveRow,
            Text::new("OBJECTIVES"),
            font(16.0),
            TextColor(theme.secondary_text),
        ));
        for (objective, state) in objectives.objectives.iter().zip(tracker.states()) {
            parent.spawn((
                HudObjectiveRow,
                Text::new(objective.label(state)),
                font(17.0),
                TextColor(if state.completed {
                    theme.accent
                } else {
                    theme.text
                }),
            ));
        }
        if tracker.all_complete() {
            parent.spawn((
                HudObjectiveRow,
                Text::new("All objectives complete!"),
                font(18.0),
                TextColor(theme.accent),
            ));
        }
    });
}
