//! Bevy integration: loading the language packs, keeping texts and fonts in
//! step with the active language, and the main-menu language button.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::text::{Font, FontSource, Justify};
use bevy::ui::GlobalZIndex;

use crate::app::states::AppState;
use crate::stage::localization::{StageLocale, install_level_overlay};
use crate::ui::theme::UiTheme;

use super::catalog::{LanguageCatalog, SOURCE_LANGUAGE};
use super::fonts::{FontContext, FontLibrary, FontRole, role_for_size};
use super::meta::Direction;
use super::pack::{self, PackFiles};
use super::service::{I18n, LocArg, requested_language, save_language};

/// Environment variable overriding the language folder.
pub const I18N_DIR_ENV: &str = "OOPSMATH_I18N_DIR";

/// English compiled into the executable, used only when `assets/i18n/en-US` is
/// missing, so the game never shows raw message keys for its own strings.
const BUILTIN_META: &str = include_str!("../../../../assets/i18n/en-US/_meta.ron");
const BUILTIN_CORE: &str = include_str!("../../../../assets/i18n/en-US/core.ftl");

/// Where the language folder is: the override, else the workspace `assets/i18n`
/// when running under cargo, else `assets/i18n` next to the executable.
pub fn resolve_i18n_dir(
    override_dir: Option<PathBuf>,
    manifest_dir: Option<PathBuf>,
    exe_dir: Option<PathBuf>,
) -> PathBuf {
    if let Some(dir) = override_dir {
        return dir;
    }
    if let Some(manifest) = manifest_dir {
        let development = manifest.join("../../assets/i18n");
        if development.is_dir() {
            return development;
        }
    }
    exe_dir.unwrap_or_default().join("assets").join("i18n")
}

fn default_i18n_dir() -> PathBuf {
    resolve_i18n_dir(
        std::env::var_os(I18N_DIR_ENV).map(PathBuf::from),
        std::env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from),
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf)),
    )
}

fn builtin_english() -> PackFiles {
    let mut pack = PackFiles {
        origin: "built-in English".to_string(),
        ..default()
    };
    pack.files.insert(
        format!("{SOURCE_LANGUAGE}/_meta.ron"),
        BUILTIN_META.as_bytes().to_vec(),
    );
    pack.files.insert(
        format!("{SOURCE_LANGUAGE}/core.ftl"),
        BUILTIN_CORE.as_bytes().to_vec(),
    );
    pack
}

/// Text that follows the language: set `key` (and optional variables) and the
/// `Text` on the same entity is refreshed on spawn, when the component changes
/// and when the language changes.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct LocalizedText {
    pub key: String,
    pub args: Vec<(String, LocArg)>,
}

impl LocalizedText {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            args: Vec::new(),
        }
    }

    pub fn with(mut self, name: impl Into<String>, value: impl Into<LocArg>) -> Self {
        self.args.push((name.into(), value.into()));
        self
    }
}

/// Remembers that the `Text` on this entity was translated by
/// [`translate_built_texts`]: the English it came from and what was shown.
#[derive(Component, Debug, Clone)]
pub struct Translated {
    english: String,
    shown: String,
}

#[derive(Component)]
struct LanguageButton;

pub struct I18nPlugin;

impl Plugin for I18nPlugin {
    fn build(&self, app: &mut App) {
        let dir = default_i18n_dir();
        let (mut packs, mut diagnostics) = pack::read_i18n_dir(&dir);
        let mut catalog = LanguageCatalog::from_packs(&packs);
        if catalog.language(SOURCE_LANGUAGE).is_none() {
            diagnostics.push(format!(
                "{SOURCE_LANGUAGE} was not found in {}; using the built-in English",
                dir.display()
            ));
            packs.push(builtin_english());
            catalog = LanguageCatalog::from_packs(&packs);
        }
        install_level_overlay(catalog.level_overlay());

        let mut library = FontLibrary::default();
        for language in catalog.languages() {
            for (id, bytes) in &language.fonts {
                library.add_bytes(id, bytes.clone());
            }
        }
        if let Some(assets) = dir.parent() {
            library.scan_dir(&assets.join("fonts"));
        }

        let i18n = I18n::new(catalog, &requested_language());
        for message in diagnostics
            .iter()
            .map(String::as_str)
            .chain(i18n.diagnostics())
        {
            warn!("i18n: {message}");
        }
        info!(
            "i18n: language '{}' ({} installed, folder {})",
            i18n.active_code(),
            i18n.languages().len(),
            dir.display()
        );

        app.insert_resource(StageLocale {
            locale: i18n.active_code().to_string(),
            fallback: SOURCE_LANGUAGE.to_string(),
        })
        .insert_resource(i18n)
        .insert_resource(library)
        .add_systems(OnEnter(AppState::MainMenu), spawn_language_button)
        .add_systems(Update, handle_language_button)
        .add_systems(
            PostUpdate,
            (
                refresh_localized_texts,
                translate_built_texts,
                apply_fonts_to_new_texts,
                apply_fonts_to_all_texts.run_if(resource_changed::<I18n>),
                apply_direction_to_new_layouts,
                apply_direction_to_all_layouts.run_if(resource_changed::<I18n>),
            )
                .chain(),
        );
    }
}

/// Fills `Text` from a [`LocalizedText`] key.
fn refresh_localized_texts(i18n: Res<I18n>, mut texts: Query<(Ref<LocalizedText>, &mut Text)>) {
    let language_changed = i18n.is_changed();
    for (localized, mut text) in &mut texts {
        if language_changed || localized.is_changed() {
            let value = i18n.format(&localized.key, &localized.args);
            if text.0 != value {
                text.0 = value;
            }
        }
    }
}

/// Translates text the game builds in code (`format!("Placed: {n} / {max}")`),
/// see [`super::templates`]. It handles every `Text` without a
/// [`LocalizedText`]:
///
/// * new or rewritten text (by any system) that is recognised is translated;
/// * on a language change, text it translated earlier is translated again from
///   its English, so everything on screen follows the language;
/// * text a system rewrote in the meantime counts as new text.
fn translate_built_texts(
    mut commands: Commands,
    i18n: Res<I18n>,
    mut texts: Query<(Entity, &mut Text, Option<&Translated>), Without<LocalizedText>>,
) {
    let language_changed = i18n.is_changed();
    for (entity, mut text, translated) in &mut texts {
        let own_text = translated.is_some_and(|t| t.shown == text.0);
        if own_text {
            if language_changed && let Some(t) = translated {
                // Back to English when the language lacks the message.
                let shown = i18n
                    .translate_text(&t.english)
                    .unwrap_or_else(|| t.english.clone());
                if text.0 != shown {
                    text.0 = shown.clone();
                }
                commands.entity(entity).insert(Translated {
                    english: t.english.clone(),
                    shown,
                });
            }
            continue;
        }
        if !translated.is_some() && !text.is_changed() {
            continue;
        }
        match i18n.translate_text(&text.0) {
            Some(shown) => {
                let english = text.0.clone();
                if text.0 != shown {
                    text.0 = shown.clone();
                }
                commands
                    .entity(entity)
                    .insert(Translated { english, shown });
            }
            None if translated.is_some() => {
                commands.entity(entity).remove::<Translated>();
            }
            None => {}
        }
    }
}

fn wanted_font(
    i18n: &I18n,
    library: &mut FontLibrary,
    assets: &mut Assets<Font>,
    font: &TextFont,
    role: Option<&FontRole>,
    context: Option<&FontContext>,
) -> FontSource {
    let size = match font.font_size {
        FontSize::Px(px) => px,
        _ => 16.0,
    };
    let role = role.copied().unwrap_or_else(|| role_for_size(size));
    i18n.font_spec()
        .and_then(|spec| spec.resolve(role, context.map(|c| c.0.as_str())))
        .and_then(|id| library.handle(id, assets))
        .map(FontSource::Handle)
        .unwrap_or_default()
}

#[allow(clippy::type_complexity)]
fn apply_fonts_to_new_texts(
    i18n: Res<I18n>,
    mut library: ResMut<FontLibrary>,
    mut assets: ResMut<Assets<Font>>,
    mut texts: Query<
        (&mut TextFont, Option<&FontRole>, Option<&FontContext>),
        Or<(Added<TextFont>, Changed<TextFont>)>,
    >,
) {
    for (mut font, role, context) in &mut texts {
        let wanted = wanted_font(&i18n, &mut library, &mut assets, &font, role, context);
        if font.font != wanted {
            font.font = wanted;
        }
    }
}

fn apply_fonts_to_all_texts(
    i18n: Res<I18n>,
    mut library: ResMut<FontLibrary>,
    mut assets: ResMut<Assets<Font>>,
    mut texts: Query<(&mut TextFont, Option<&FontRole>, Option<&FontContext>)>,
) {
    for (mut font, role, context) in &mut texts {
        let wanted = wanted_font(&i18n, &mut library, &mut assets, &font, role, context);
        if font.font != wanted {
            font.font = wanted;
        }
    }
}

/// Left-aligned text becomes start-aligned (right-aligned for right-to-left
/// text) and back. Layout of panels is not mirrored.
fn aligned(justify: Justify, direction: Direction) -> Justify {
    match (direction, justify) {
        (Direction::Rtl, Justify::Left) => Justify::Start,
        (Direction::Ltr, Justify::Start) => Justify::Left,
        (_, other) => other,
    }
}

fn apply_direction_to_new_layouts(
    i18n: Res<I18n>,
    mut layouts: Query<&mut TextLayout, Added<TextLayout>>,
) {
    for mut layout in &mut layouts {
        let wanted = aligned(layout.justify, i18n.direction());
        if layout.justify != wanted {
            layout.justify = wanted;
        }
    }
}

fn apply_direction_to_all_layouts(i18n: Res<I18n>, mut layouts: Query<&mut TextLayout>) {
    for mut layout in &mut layouts {
        let wanted = aligned(layout.justify, i18n.direction());
        if layout.justify != wanted {
            layout.justify = wanted;
        }
    }
}

fn language_label(i18n: &I18n) -> LocalizedText {
    let native = i18n
        .active_meta()
        .map(|m| m.native_name.clone())
        .unwrap_or_else(|| i18n.active_code().to_string());
    LocalizedText::new("menu-language").with("name", native)
}

/// The language button in the corner of the main menu.
fn spawn_language_button(mut commands: Commands, i18n: Res<I18n>, theme: Res<UiTheme>) {
    if i18n.languages().len() < 2 {
        return;
    }
    commands.spawn((
        LanguageButton,
        Button,
        Node {
            position_type: PositionType::Absolute,
            right: px(24),
            bottom: px(24),
            padding: UiRect::axes(px(18), px(10)),
            ..default()
        },
        BackgroundColor(theme.button),
        Text::new(""),
        language_label(&i18n),
        TextFont {
            font_size: FontSize::Px(18.0),
            ..default()
        },
        FontRole::Button,
        TextColor(theme.text),
        GlobalZIndex(5),
        DespawnOnExit(AppState::MainMenu),
    ));
}

/// Cycles the language, remembers the choice and updates the stage locale.
fn handle_language_button(
    pressed: Query<&Interaction, (Changed<Interaction>, With<LanguageButton>)>,
    mut labels: Query<&mut LocalizedText, With<LanguageButton>>,
    mut i18n: ResMut<I18n>,
    mut stage_locale: ResMut<StageLocale>,
) {
    if !pressed.iter().any(|i| *i == Interaction::Pressed) {
        return;
    }
    let code = i18n.cycle_language();
    stage_locale.locale = code.clone();
    save_language(&code);
    let label = language_label(&i18n);
    for mut localized in &mut labels {
        *localized = label.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_language_folder_is_found_in_the_expected_order() {
        let base = std::env::temp_dir().join(format!("oopsmath_i18n_dir_{}", std::process::id()));
        let manifest = base.join("crates").join("game");
        let dev = base.join("assets").join("i18n");
        std::fs::create_dir_all(&manifest).unwrap();
        std::fs::create_dir_all(&dev).unwrap();
        let exe = base.join("bin");

        let override_dir = PathBuf::from("/custom");
        assert_eq!(
            resolve_i18n_dir(
                Some(override_dir.clone()),
                Some(manifest.clone()),
                Some(exe.clone())
            ),
            override_dir
        );
        let found = resolve_i18n_dir(None, Some(manifest), Some(exe.clone()));
        assert!(found.is_dir() && found.ends_with("assets/i18n"));
        // Not under cargo: next to the executable.
        assert_eq!(
            resolve_i18n_dir(None, None, Some(exe.clone())),
            exe.join("assets").join("i18n")
        );
        // Under cargo but without the workspace folder: still next to the executable.
        let lonely = base.join("elsewhere");
        std::fs::create_dir_all(&lonely).unwrap();
        assert_eq!(
            resolve_i18n_dir(None, Some(lonely), Some(exe.clone())),
            exe.join("assets").join("i18n")
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn localized_text_collects_variables() {
        let text = LocalizedText::new("buy").with("n", 3).with("who", "Sam");
        assert_eq!(text.key, "buy");
        assert_eq!(
            text.args,
            [
                ("n".to_string(), LocArg::Int(3)),
                ("who".to_string(), LocArg::Text("Sam".into()))
            ]
        );
    }

    #[test]
    fn right_to_left_text_is_start_aligned_and_restored() {
        assert_eq!(aligned(Justify::Left, Direction::Rtl), Justify::Start);
        assert_eq!(aligned(Justify::Start, Direction::Ltr), Justify::Left);
        assert_eq!(aligned(Justify::Center, Direction::Rtl), Justify::Center);
        assert_eq!(aligned(Justify::Left, Direction::Ltr), Justify::Left);
    }

    #[test]
    fn the_built_in_english_is_a_valid_language() {
        let catalog = LanguageCatalog::from_packs(&[builtin_english()]);
        assert!(catalog.diagnostics.is_empty(), "{:?}", catalog.diagnostics);
        let i18n = I18n::new(catalog, "en-US");
        assert_eq!(i18n.tr("session-resume"), "Resume");
        assert_eq!(i18n.translate_text("PLAY").as_deref(), Some("PLAY"));
    }
}
