//! Stage browser: a scrollable list of discovered stages plus a details panel.
//!
//! The screen owns only presentation and interaction. Discovery lives in
//! [`crate::stage::catalog`], localization in [`crate::stage::localization`],
//! and the load itself in [`crate::app::loading`]. Selecting an entry only
//! updates [`StageSelection`]; loading happens when Play Stage is pressed.

use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;

use crate::app::{
    stages::{StageCatalogResource, StageLoadFailure, StageSelection},
    states::AppState,
};
use crate::stage::catalog::{StageCatalog, StageCatalogEntry};
use crate::stage::localization::StageLocale;
use crate::ui::{
    common::button,
    components::{
        StageBrowserRoot, StageDetailsText, StageListContainer, StageListEntry, StageStatusText,
        UiAction, UiButton,
    },
    theme::UiTheme,
};

/// Spawns the browser frame. Called once on entering [`AppState::StageBrowser`];
/// [`refresh`] fills the list and details on the same frame.
pub fn spawn(
    mut commands: Commands,
    mut catalog: ResMut<StageCatalogResource>,
    mut selection: ResMut<StageSelection>,
    theme: Res<UiTheme>,
) {
    let scanned = StageCatalog::discover_default();
    reset_selection(&scanned, &mut selection);
    catalog.0 = scanned;

    commands
        .spawn((
            StageBrowserRoot,
            DespawnOnExit(AppState::StageBrowser),
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(16)),
                row_gap: px(12),
                ..default()
            },
            BackgroundColor(theme.background),
        ))
        .with_children(|root| {
            // Header: page title plus a live status line.
            root.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                ..default()
            })
            .with_children(|header| {
                header.spawn((
                    Text::new("SELECT A STAGE"),
                    TextFont {
                        font_size: FontSize::Px(34.0),
                        ..default()
                    },
                    TextColor(theme.text),
                ));
                header.spawn((
                    StageStatusText,
                    Text::new("Scanning..."),
                    TextFont {
                        font_size: FontSize::Px(15.0),
                        ..default()
                    },
                    TextColor(theme.secondary_text),
                ));
            });

            // Body: stage list (left) and details (right).
            root.spawn(Node {
                flex_grow: 1.0,
                flex_direction: FlexDirection::Row,
                column_gap: px(12),
                min_height: px(0),
                ..default()
            })
            .with_children(|body| {
                body.spawn((
                    StageListContainer,
                    Node {
                        width: percent(38),
                        min_width: px(240),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(6),
                        padding: UiRect::all(px(10)),
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                    BackgroundColor(theme.panel),
                ));

                body.spawn((
                    Node {
                        flex_grow: 1.0,
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(px(12)),
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                    BackgroundColor(theme.panel),
                ))
                .with_child((
                    StageDetailsText,
                    Text::new("Select a stage to see its details."),
                    TextFont {
                        font_size: FontSize::Px(17.0),
                        ..default()
                    },
                    TextColor(theme.text),
                ));
            });

            // Footer: navigation and the Play Stage action.
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: px(10),
                ..default()
            })
            .with_children(|footer| {
                footer
                    .spawn(button(&theme, UiAction::Back, 130.0, 52.0))
                    .with_child(nav_text(theme.text, "BACK"));
                footer
                    .spawn(button(&theme, UiAction::Rescan, 150.0, 52.0))
                    .with_child(nav_text(theme.text, "RESCAN"));
                footer
                    .spawn(button(&theme, UiAction::PlaySelected, 190.0, 52.0))
                    .with_child(nav_text(theme.text, "PLAY STAGE"));
            });
        });
}

fn nav_text(color: Color, label: &str) -> (Text, TextFont, TextColor) {
    (
        Text::new(label),
        TextFont {
            font_size: FontSize::Px(20.0),
            ..default()
        },
        TextColor(color),
    )
}

/// Rebuilds the selectable list whenever the catalog or the selection changes.
/// The change check keeps this off the per-frame hot path.
pub fn rebuild_list(
    mut commands: Commands,
    catalog: Res<StageCatalogResource>,
    selection: Res<StageSelection>,
    locale: Res<StageLocale>,
    theme: Res<UiTheme>,
    containers: Query<Entity, With<StageListContainer>>,
    entries: Query<Entity, With<StageListEntry>>,
) {
    if !catalog.is_changed() && !selection.is_changed() {
        return;
    }

    for entity in &entries {
        commands.entity(entity).despawn();
    }
    if let Ok(container) = containers.single() {
        spawn_entries(
            &mut commands,
            container,
            &catalog.0,
            selection.index,
            &locale,
            &theme,
        );
    }
}

/// Updates the details panel when the catalog or selection changes.
pub fn update_details(
    catalog: Res<StageCatalogResource>,
    selection: Res<StageSelection>,
    locale: Res<StageLocale>,
    mut details: Query<&mut Text, With<StageDetailsText>>,
) {
    if !catalog.is_changed() && !selection.is_changed() {
        return;
    }
    if let Ok(mut text) = details.single_mut() {
        *text = Text::new(details_body(&catalog.0, selection.index, &locale));
    }
}

/// Updates the header status line when the catalog or failure changes.
pub fn update_status(
    catalog: Res<StageCatalogResource>,
    failure: Res<StageLoadFailure>,
    mut status: Query<&mut Text, With<StageStatusText>>,
) {
    if !catalog.is_changed() && !failure.is_changed() {
        return;
    }
    if let Ok(mut text) = status.single_mut() {
        *text = Text::new(status_body(&catalog.0, &failure));
    }
}

/// Handles every stage-browser button.
pub fn handle_buttons(
    mut interactions: Query<(&Interaction, &UiButton), (Changed<Interaction>, With<Button>)>,
    mut catalog: ResMut<StageCatalogResource>,
    mut selection: ResMut<StageSelection>,
    mut failure: ResMut<StageLoadFailure>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for (interaction, ui_button) in &mut interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if ui_button.action == UiAction::Rescan {
            rescan(&mut catalog, &mut selection, &mut failure);
            continue;
        }
        if let Some(next) = apply_action(ui_button.action, &catalog.0, &mut selection, &mut failure)
        {
            next_state.set(next);
        }
    }
}

/// Applies a browser action to the selection/failure state and returns the
/// state transition it should trigger, if any.
///
/// Pure apart from the resources it mutates, so the navigation rules are unit
/// testable without a graphical window.
fn apply_action(
    action: UiAction,
    catalog: &StageCatalog,
    selection: &mut StageSelection,
    failure: &mut StageLoadFailure,
) -> Option<AppState> {
    match action {
        UiAction::SelectStage(index) => {
            if let Some(entry) = catalog.get(index) {
                selection.select(index, entry.dat_path.clone());
            }
            None
        }
        UiAction::PlaySelected => {
            if selection.dat_path.is_some() {
                Some(AppState::LoadingStage)
            } else {
                failure.0 = Some("Select a stage before pressing Play Stage.".to_string());
                None
            }
        }
        UiAction::Back => Some(AppState::MainMenu),
        // Rescan is handled before this call; `Play` belongs to the main menu.
        UiAction::Rescan | UiAction::Play => None,
    }
}

/// Keeps the stage list scrollable with the mouse wheel.
pub fn scroll_list(
    mut wheel: MessageReader<MouseWheel>,
    mut list: Query<&mut ScrollPosition, With<StageListContainer>>,
) {
    let mut delta = 0.0;
    for event in wheel.read() {
        delta += event.y
            * match event.unit {
                bevy::input::mouse::MouseScrollUnit::Line => 24.0,
                bevy::input::mouse::MouseScrollUnit::Pixel => 1.0,
            };
    }
    if delta == 0.0 {
        return;
    }
    for mut position in &mut list {
        position.0.y = (position.0.y - delta).max(0.0);
    }
}

/// Re-scans the runtime directory, preserving the selection when the same
/// package is still present.
fn rescan(
    catalog: &mut StageCatalogResource,
    selection: &mut StageSelection,
    failure: &mut StageLoadFailure,
) {
    let previous_path = selection.dat_path.clone();
    let rescan_dir = catalog.0.dir.clone();
    let scanned = StageCatalog::discover(&rescan_dir);

    let restored = previous_path.as_ref().and_then(|path| {
        scanned
            .entries
            .iter()
            .position(|entry| &entry.dat_path == path)
    });

    match restored {
        Some(index) => {
            if let Some(entry) = scanned.get(index) {
                selection.select(index, entry.dat_path.clone());
            }
        }
        None => reset_selection(&scanned, selection),
    }
    *catalog = StageCatalogResource(scanned);
    failure.0 = None;
}

/// Points the selection at the first entry, or clears it when there is none.
fn reset_selection(catalog: &StageCatalog, selection: &mut StageSelection) {
    match catalog.get(0) {
        Some(entry) => selection.select(0, entry.dat_path.clone()),
        None => selection.clear(),
    }
}

/// Spawns one selectable entry per catalog stage.
fn spawn_entries(
    commands: &mut Commands,
    container: Entity,
    catalog: &StageCatalog,
    selected: Option<usize>,
    locale: &StageLocale,
    theme: &UiTheme,
) {
    commands.entity(container).with_children(|parent| {
        for (index, entry) in catalog.entries.iter().enumerate() {
            let is_selected = selected == Some(index);
            let background = if is_selected {
                theme.selected
            } else {
                theme.button
            };
            parent
                .spawn((
                    Button,
                    UiButton {
                        action: UiAction::SelectStage(index),
                    },
                    StageListEntry { index },
                    Node {
                        width: percent(100),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(2),
                        padding: UiRect::all(px(10)),
                        ..default()
                    },
                    BackgroundColor(background),
                ))
                .with_children(|entry_node| {
                    entry_node.spawn((
                        Text::new(entry_title_line(entry, locale, is_selected)),
                        TextFont {
                            font_size: FontSize::Px(20.0),
                            ..default()
                        },
                        TextColor(theme.text),
                    ));
                    entry_node.spawn((
                        Text::new(entry_summary_line(entry)),
                        TextFont {
                            font_size: FontSize::Px(14.0),
                            ..default()
                        },
                        TextColor(theme.secondary_text),
                    ));
                });
        }
    });
}

/// Title shown in the list, prefixed with a marker when selected so the
/// selection stays visible even while the entry is hovered.
fn entry_title_line(entry: &StageCatalogEntry, locale: &StageLocale, selected: bool) -> String {
    let title = entry.translated_title(locale).unwrap_or(&entry.stage_id);
    if selected {
        format!("> {title}")
    } else {
        title.to_string()
    }
}

/// Compact summary line under the title.
fn entry_summary_line(entry: &StageCatalogEntry) -> String {
    let mut parts = Vec::new();
    if let Some(level) = entry.level {
        parts.push(format!("Level {level}"));
    }
    if let Some(difficulty) = &entry.difficulty {
        parts.push(difficulty.clone());
    }
    if let Some(grade) = grade_range(entry) {
        parts.push(grade);
    }
    let mut line = parts.join("  |  ");
    if !entry.topics.is_empty() {
        if !line.is_empty() {
            line.push('\n');
        }
        line.push_str(&entry.topics.join(", "));
    }
    line
}

/// The details panel body for the selected entry.
fn details_body(catalog: &StageCatalog, selected: Option<usize>, locale: &StageLocale) -> String {
    if catalog.is_empty() {
        return "No valid stage packages were found.\n\nPut compiled `.dat` files in the stages \
                directory, then press Rescan."
            .to_string();
    }
    let Some(entry) = selected.and_then(|index| catalog.get(index)) else {
        return "Select a stage to see its details.".to_string();
    };

    let mut out = String::new();
    out.push_str(entry.translated_title(locale).unwrap_or(&entry.stage_id));
    out.push('\n');
    if let Some(description) = entry.translated_description(locale) {
        out.push_str(description);
        out.push('\n');
    }

    out.push_str("\nOVERVIEW\n");
    field(&mut out, "ID", Some(entry.stage_id.clone()));
    field(
        &mut out,
        "Level",
        entry.level.map(|level| level.to_string()),
    );
    field(&mut out, "Difficulty", entry.difficulty.clone());
    field(&mut out, "Grades", grade_range(entry));
    if !entry.topics.is_empty() {
        field(&mut out, "Topics", Some(entry.topics.join(", ")));
    }
    if !entry.locales.is_empty() {
        field(&mut out, "Locales", Some(entry.locales.join(", ")));
    }

    if let Some(question) = entry.translated_question(locale) {
        out.push_str("\nLEARNING\n");
        field(&mut out, "Question", Some(question.to_string()));
    }

    let mut environment = Vec::new();
    if let Some(preset) = &entry.environment_preset {
        environment.push(("Preset", preset.clone()));
    }
    if let Some(size) = entry.world_size {
        environment.push((
            "World size",
            format!("{} x {} x {}", size[0], size[1], size[2]),
        ));
    }
    if let Some(origin) = entry.world_origin {
        environment.push((
            "World origin",
            format!("({}, {}, {})", origin[0], origin[1], origin[2]),
        ));
    }
    if !environment.is_empty() {
        out.push_str("\nENVIRONMENT\n");
        for (label, value) in environment {
            field(&mut out, label, Some(value));
        }
    }

    if let Some(tasks) = entry.objective_task_count {
        out.push_str("\nOBJECTIVES\n");
        field(&mut out, "Tasks", Some(tasks.to_string()));
    }

    if entry.reward_money.is_some() || entry.reward_xp.is_some() {
        out.push_str("\nREWARDS\n");
        field(
            &mut out,
            "Money",
            entry.reward_money.map(|money| money.to_string()),
        );
        field(&mut out, "XP", entry.reward_xp.map(|xp| xp.to_string()));
    }

    out
}

/// Appends a `Label: value` line when the value is available.
fn field(out: &mut String, label: &str, value: Option<String>) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        out.push_str(label);
        out.push_str(": ");
        out.push_str(&value);
        out.push('\n');
    }
}

fn grade_range(entry: &StageCatalogEntry) -> Option<String> {
    match (entry.grade_min, entry.grade_max) {
        (Some(min), Some(max)) if min == max => Some(format!("Grade {min}")),
        (Some(min), Some(max)) => Some(format!("Grades {min}-{max}")),
        (Some(min), None) => Some(format!("Grade {min}+")),
        (None, Some(max)) => Some(format!("Up to grade {max}")),
        (None, None) => None,
    }
}

/// The header status line: counts, load failure and per-file diagnostics.
fn status_body(catalog: &StageCatalog, failure: &StageLoadFailure) -> String {
    let mut out = format!(
        "{} stage(s) found in {}",
        catalog.len(),
        catalog.dir.display()
    );
    if let Some(message) = &failure.0 {
        out.push('\n');
        out.push_str("[!] ");
        out.push_str(message);
    }
    for diagnostic in &catalog.diagnostics {
        out.push('\n');
        out.push_str("[!] ");
        out.push_str(&diagnostic.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::stage::catalog::{CatalogDiagnostic, CatalogDiagnosticKind};
    use crate::stage::localization::StageMessages;
    use crate::stage::package::Localization;

    fn messages(locale: &str, text: &str) -> StageMessages {
        StageMessages::from_localizations(&[Localization {
            locale: locale.to_string(),
            raw: text.as_bytes().to_vec(),
        }])
    }

    fn entry(id: &str) -> StageCatalogEntry {
        let text = format!(
            "stage.{id}.title = Title {id}\nstage.{id}.description = Desc {id}\nstage.{id}.question = Q {id}"
        );
        StageCatalogEntry {
            stage_id: id.to_string(),
            dat_path: PathBuf::from(format!("{id}.dat")),
            title_key: format!("stage.{id}.title"),
            description_key: Some(format!("stage.{id}.description")),
            level: Some(1),
            difficulty: Some("tutorial".to_string()),
            grade_min: Some(7),
            grade_max: Some(8),
            topics: vec!["multiplication".to_string()],
            locales: vec!["en-US".to_string()],
            question_key: Some(format!("stage.{id}.question")),
            environment_preset: Some("meadow".to_string()),
            world_size: Some([16, 8, 16]),
            world_origin: Some([0, 0, 0]),
            objective_task_count: Some(2),
            reward_money: Some(100),
            reward_xp: Some(10),
            messages: messages("en-US", &text),
        }
    }

    fn catalog(entries: Vec<StageCatalogEntry>) -> StageCatalog {
        StageCatalog {
            entries,
            diagnostics: Vec::new(),
            dir: PathBuf::from("stages"),
        }
    }

    #[test]
    fn selecting_an_entry_updates_selection_and_details() {
        let catalog = catalog(vec![entry("a"), entry("b")]);
        let mut selection = StageSelection::default();
        let mut failure = StageLoadFailure::default();
        let locale = StageLocale::from("en-US");

        let next = apply_action(
            UiAction::SelectStage(1),
            &catalog,
            &mut selection,
            &mut failure,
        );
        assert!(next.is_none());
        assert_eq!(selection.index, Some(1));
        assert_eq!(selection.dat_path, Some(PathBuf::from("b.dat")));

        let details = details_body(&catalog, selection.index, &locale);
        assert!(details.contains("Title b"));
        assert!(details.contains("Desc b"));
        assert!(details.contains("Q b"));
        assert!(!details.contains("Title a"));
    }

    #[test]
    fn back_returns_to_the_main_menu() {
        let catalog = catalog(vec![entry("a")]);
        let mut selection = StageSelection::default();
        let mut failure = StageLoadFailure::default();
        let next = apply_action(UiAction::Back, &catalog, &mut selection, &mut failure);
        assert_eq!(next, Some(AppState::MainMenu));
    }

    #[test]
    fn play_without_a_selection_reports_a_failure() {
        let catalog = catalog(vec![entry("a")]);
        let mut selection = StageSelection::default();
        let mut failure = StageLoadFailure::default();
        let next = apply_action(
            UiAction::PlaySelected,
            &catalog,
            &mut selection,
            &mut failure,
        );
        assert_eq!(next, None);
        assert!(failure.0.is_some());
    }

    #[test]
    fn play_with_a_selection_enters_the_loading_state() {
        let catalog = catalog(vec![entry("a")]);
        let mut selection = StageSelection::default();
        let mut failure = StageLoadFailure::default();
        apply_action(
            UiAction::SelectStage(0),
            &catalog,
            &mut selection,
            &mut failure,
        );
        let next = apply_action(
            UiAction::PlaySelected,
            &catalog,
            &mut selection,
            &mut failure,
        );
        assert_eq!(next, Some(AppState::LoadingStage));
        assert!(failure.0.is_none());
    }

    #[test]
    fn details_report_intentional_empty_and_no_selection_states() {
        let locale = StageLocale::from("en-US");
        assert!(details_body(&catalog(vec![]), None, &locale).contains("No valid stage"));
        assert!(details_body(&catalog(vec![entry("a")]), None, &locale).contains("Select a stage"));
    }

    #[test]
    fn details_omit_missing_optional_fields() {
        let mut bare = entry("bare");
        bare.description_key = None;
        bare.question_key = None;
        bare.difficulty = None;
        bare.grade_min = None;
        bare.grade_max = None;
        bare.environment_preset = None;
        bare.world_size = None;
        bare.world_origin = None;
        bare.objective_task_count = None;
        bare.reward_money = None;
        bare.reward_xp = None;
        bare.topics.clear();

        let details = details_body(&catalog(vec![bare]), Some(0), &StageLocale::from("en-US"));
        assert!(details.contains("Title bare"));
        assert!(!details.contains("ENVIRONMENT"));
        assert!(!details.contains("OBJECTIVES"));
        assert!(!details.contains("REWARDS"));
        assert!(!details.contains("LEARNING"));
    }

    #[test]
    fn status_lists_counts_diagnostics_and_failure() {
        let mut catalog = catalog(vec![entry("a")]);
        catalog.diagnostics.push(CatalogDiagnostic {
            path: PathBuf::from("stages/broken.dat"),
            kind: CatalogDiagnosticKind::PackageRejected,
            message: "bad header".to_string(),
        });
        let failure = StageLoadFailure(Some("Failed to load 'a.dat'".to_string()));
        let status = status_body(&catalog, &failure);
        assert!(status.contains("1 stage(s)"));
        assert!(status.contains("broken.dat"));
        assert!(status.contains("bad header"));
        assert!(status.contains("Failed to load 'a.dat'"));
    }

    #[test]
    fn grade_range_formats_every_shape() {
        let mut e = entry("a");
        assert_eq!(grade_range(&e), Some("Grades 7-8".to_string()));
        e.grade_max = Some(7);
        assert_eq!(grade_range(&e), Some("Grade 7".to_string()));
        e.grade_max = None;
        assert_eq!(grade_range(&e), Some("Grade 7+".to_string()));
        e.grade_min = None;
        e.grade_max = Some(9);
        assert_eq!(grade_range(&e), Some("Up to grade 9".to_string()));
        e.grade_max = None;
        assert_eq!(grade_range(&e), None);
    }

    #[test]
    fn reset_selection_picks_the_first_entry_or_clears() {
        let mut selection = StageSelection::default();
        reset_selection(&catalog(vec![entry("a"), entry("b")]), &mut selection);
        assert_eq!(selection.index, Some(0));
        assert_eq!(selection.dat_path, Some(PathBuf::from("a.dat")));

        reset_selection(&catalog(vec![]), &mut selection);
        assert_eq!(selection.index, None);
        assert_eq!(selection.dat_path, None);
    }

    #[test]
    fn entry_summary_includes_level_difficulty_and_topics() {
        let summary = entry_summary_line(&entry("a"));
        assert!(summary.contains("Level 1"));
        assert!(summary.contains("tutorial"));
        assert!(summary.contains("multiplication"));
    }
}
