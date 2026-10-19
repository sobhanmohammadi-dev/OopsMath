//! Maps a loaded stage onto what the gameplay HUD shows.
//!
//! Everything here is plain data and pure functions (no Bevy rendering), so it
//! is unit tested without a window. Sources of truth:
//! * title / description: the stage's localization keys, resolved with
//!   [`StageLocale`] (nothing is shown for an untranslated description);
//! * allowed blocks: `construction.allowed_blocks`, parsed by
//!   [`parse_allowed_blocks`];
//! * block name and price: the [`BlockRegistry`] definition, with the stage's
//!   `cost` taking precedence over the default price.
//!
//! Stage block ids that are not in the registry (the sample stage uses
//! `brick`) are kept and reported as unavailable, never dropped or faked.
//! The stage loader does not validate block ids, so this is where the
//! mismatch becomes visible.

use bevy::prelude::*;

use crate::blocks::stage_rules::{StageBlockRule, parse_allowed_blocks};
use crate::blocks::{BlockId, BlockRegistry};
use crate::items::tools::{ToolId, parse_allowed_tools};
use crate::stage::localization::{StageLocale, StageMessages};
use crate::stage::package::StagePackage;

use serde_json::Value;

/// The HUD's view of the active stage; `None` while no stage is loaded.
#[derive(Resource, Default, Debug)]
pub struct StageHud {
    pub model: Option<StageHudModel>,
    /// Set when the model must be rebuilt even if no input resource changed
    /// (e.g. on entering the game).
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageHudModel {
    /// Localized title, or the raw key when untranslated (existing convention).
    pub title: String,
    /// Localized description; `None` when not declared or not translated.
    pub description: Option<String>,
    pub blocks: BlockList,
    /// `construction.allowed_tools`, or why the list could not be read.
    pub allowed_tools: Result<Vec<ToolId>, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockList {
    /// The stage declares no allowed blocks.
    Empty,
    /// `construction.allowed_blocks` is malformed.
    Invalid(String),
    Entries(Vec<HudBlock>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HudBlock {
    pub id: BlockId,
    pub status: HudBlockStatus,
    /// Stage minimum; only displayed when above zero.
    pub min_count: u32,
    /// Stage maximum, present only when the stage declares one.
    pub max_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HudBlockStatus {
    /// Registered: selectable. `price` is the stage cost if given, otherwise
    /// the registry default, in coins.
    Available { name: String, price: u32 },
    /// The stage lists an id the registry does not know.
    Unregistered,
}

impl HudBlock {
    pub fn is_selectable(&self) -> bool {
        matches!(self.status, HudBlockStatus::Available { .. })
    }

    /// One-line row text; `selected` adds the same `> ` marker the stage
    /// browser uses, which stays visible while the row is hovered.
    pub fn label(&self, selected: bool) -> String {
        match &self.status {
            HudBlockStatus::Available { name, price } => {
                let marker = if selected { "> " } else { "" };
                let limits = match (self.min_count, self.max_count) {
                    (0, None) => String::new(),
                    (0, Some(max)) => format!(" (max {max})"),
                    (min, None) => format!(" (min {min})"),
                    (min, Some(max)) => format!(" (min {min}, max {max})"),
                };
                format!("{marker}{name} - {price} coins{limits}")
            }
            HudBlockStatus::Unregistered => {
                format!("{} - unavailable: not a registered block", self.id)
            }
        }
    }
}

impl StageHudModel {
    pub fn from_package(
        package: &StagePackage,
        registry: &BlockRegistry,
        locale: &StageLocale,
    ) -> Self {
        let messages = StageMessages::from_localizations(&package.localization);
        Self::from_parts(
            &package.meta.title,
            package.meta.description.as_deref(),
            &messages,
            &package.stage.construction,
            registry,
            locale,
        )
    }

    /// Core mapping, independent of `StagePackage` so tests stay small.
    pub fn from_parts(
        title_key: &str,
        description_key: Option<&str>,
        messages: &StageMessages,
        construction: &Value,
        registry: &BlockRegistry,
        locale: &StageLocale,
    ) -> Self {
        let title = locale.resolve(messages, title_key).to_string();
        let description = description_key
            .and_then(|key| messages.resolve(key, &locale.locale, &locale.fallback))
            .map(str::to_owned);
        let blocks = match parse_allowed_blocks(construction) {
            Err(err) => BlockList::Invalid(err.to_string()),
            Ok(rules) if rules.is_empty() => BlockList::Empty,
            Ok(rules) => {
                BlockList::Entries(rules.iter().map(|rule| hud_block(rule, registry)).collect())
            }
        };
        let allowed_tools = parse_allowed_tools(construction).map_err(|err| err.to_string());
        Self {
            title,
            description,
            blocks,
            allowed_tools,
        }
    }

    pub fn entries(&self) -> &[HudBlock] {
        match &self.blocks {
            BlockList::Entries(entries) => entries,
            _ => &[],
        }
    }

    pub fn find(&self, id: &BlockId) -> Option<&HudBlock> {
        self.entries().iter().find(|block| &block.id == id)
    }

    /// True only for blocks that are both listed by the stage and registered.
    pub fn is_selectable(&self, id: &BlockId) -> bool {
        self.find(id).is_some_and(HudBlock::is_selectable)
    }

    /// The selection to keep after the stage data changed: the current one if
    /// it is still selectable, otherwise nothing. No block is auto-picked.
    pub fn reconcile(&self, current: Option<&BlockId>) -> Option<BlockId> {
        current.filter(|id| self.is_selectable(id)).cloned()
    }

    pub fn allows_tool(&self, id: &ToolId) -> bool {
        self.allowed_tools
            .as_ref()
            .is_ok_and(|tools| tools.contains(id))
    }

    /// Message when the stage's tool list is malformed.
    pub fn tools_message(&self) -> Option<String> {
        self.allowed_tools
            .as_ref()
            .err()
            .map(|err| format!("The stage's tool list is invalid: {err}"))
    }

    /// `Selected: <name>` / `Selected: none`.
    pub fn selection_line(&self, selected: Option<&BlockId>) -> String {
        let name = selected.and_then(|id| match &self.find(id)?.status {
            HudBlockStatus::Available { name, .. } => Some(name.as_str()),
            HudBlockStatus::Unregistered => None,
        });
        format!("Selected: {}", name.unwrap_or("none"))
    }

    /// Explanation shown above (or instead of) the rows when the list needs one.
    pub fn list_message(&self) -> Option<String> {
        match &self.blocks {
            BlockList::Empty => Some("This stage has no selectable blocks.".to_string()),
            BlockList::Invalid(err) => Some(format!("The stage's block list is invalid: {err}")),
            BlockList::Entries(entries) if !entries.iter().any(HudBlock::is_selectable) => Some(
                "None of this stage's blocks are registered, so none can be selected.".to_string(),
            ),
            BlockList::Entries(_) => None,
        }
    }
}

fn hud_block(rule: &StageBlockRule, registry: &BlockRegistry) -> HudBlock {
    let status = match registry.get(&rule.block) {
        Some(def) => HudBlockStatus::Available {
            name: def.display_name.clone(),
            price: rule.effective_price(def),
        },
        None => HudBlockStatus::Unregistered,
    };
    HudBlock {
        id: rule.block.clone(),
        status,
        min_count: rule.min_count,
        max_count: rule.max_count,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::blocks::models::{register_builtin_blocks, wood_log_id};
    use crate::stage::package::Localization;

    fn registry() -> BlockRegistry {
        let mut registry = BlockRegistry::new();
        register_builtin_blocks(&mut registry).unwrap();
        registry
    }

    fn messages() -> StageMessages {
        StageMessages::from_localizations(&[Localization {
            locale: "en-US".into(),
            raw: b"stage.a.title = Stage A\nstage.a.description = Build it.\n".to_vec(),
        }])
    }

    fn model(description: Option<&str>, construction: Value) -> StageHudModel {
        StageHudModel::from_parts(
            "stage.a.title",
            description,
            &messages(),
            &construction,
            &registry(),
            &StageLocale::from("en-US"),
        )
    }

    #[test]
    fn title_and_description_come_from_stage_messages() {
        let m = model(Some("stage.a.description"), json!({}));
        assert_eq!(m.title, "Stage A");
        assert_eq!(m.description.as_deref(), Some("Build it."));
    }

    #[test]
    fn missing_metadata_is_not_invented() {
        // No description key declared, and one declared but untranslated.
        assert_eq!(model(None, json!({})).description, None);
        assert_eq!(model(Some("stage.a.missing"), json!({})).description, None);
        // An untranslated title shows its key (existing catalog convention).
        let m = StageHudModel::from_parts(
            "stage.zzz.title",
            None,
            &messages(),
            &json!({}),
            &registry(),
            &StageLocale::from("en-US"),
        );
        assert_eq!(m.title, "stage.zzz.title");
    }

    #[test]
    fn registered_blocks_use_registry_name_and_default_price() {
        let m = model(
            None,
            json!({"allowed_blocks": [{"id": "block.wood_log", "max_count": 20}]}),
        );
        let block = &m.entries()[0];
        assert_eq!(
            block.status,
            HudBlockStatus::Available {
                name: "Wood Log".into(),
                price: 3
            }
        );
        assert_eq!(block.label(false), "Wood Log - 3 coins (max 20)");
        assert_eq!(block.label(true), "> Wood Log - 3 coins (max 20)");
    }

    #[test]
    fn stage_cost_overrides_the_default_price() {
        let m = model(
            None,
            json!({"allowed_blocks": [{"id": "block.wood_log", "cost": 7, "min_count": 2}]}),
        );
        assert_eq!(m.entries()[0].label(false), "Wood Log - 7 coins (min 2)");
    }

    #[test]
    fn no_limits_are_shown_when_the_stage_declares_none() {
        let m = model(None, json!({"allowed_blocks": [{"id": "block.wood_log"}]}));
        assert_eq!(m.entries()[0].label(false), "Wood Log - 3 coins");
    }

    #[test]
    fn unregistered_blocks_are_reported_and_not_selectable() {
        // Mirrors the sample stage, which lists `brick`.
        let m = model(
            None,
            json!({"allowed_blocks": [{"id": "brick", "max_count": 20}]}),
        );
        let brick = BlockId::new("brick");
        assert_eq!(m.entries().len(), 1, "unknown ids are kept, not dropped");
        assert!(!m.is_selectable(&brick));
        assert_eq!(
            m.entries()[0].label(false),
            "brick - unavailable: not a registered block"
        );
        assert!(m.list_message().unwrap().contains("none can be selected"));
        assert_eq!(m.reconcile(Some(&brick)), None);
    }

    #[test]
    fn only_registered_listed_blocks_are_selectable() {
        let m = model(
            None,
            json!({"allowed_blocks": [{"id": "brick"}, {"id": "block.wood_log"}]}),
        );
        assert!(m.is_selectable(&wood_log_id()));
        assert!(!m.is_selectable(&BlockId::new("brick")));
        // Registered but not allowed by this stage.
        let other = model(None, json!({"allowed_blocks": [{"id": "brick"}]}));
        assert!(!other.is_selectable(&wood_log_id()));
        assert!(m.list_message().is_none());
    }

    #[test]
    fn empty_and_missing_lists_have_an_explicit_state() {
        for construction in [json!({}), json!({"allowed_blocks": []})] {
            let m = model(None, construction);
            assert_eq!(m.blocks, BlockList::Empty);
            assert_eq!(
                m.list_message().as_deref(),
                Some("This stage has no selectable blocks.")
            );
            assert_eq!(m.reconcile(Some(&wood_log_id())), None);
        }
    }

    #[test]
    fn malformed_lists_surface_as_invalid() {
        let m = model(None, json!({"allowed_blocks": [{"cost": 1}]}));
        assert!(matches!(m.blocks, BlockList::Invalid(_)));
        assert!(m.list_message().unwrap().contains("invalid"));
    }

    #[test]
    fn stage_changes_keep_or_clear_the_selection() {
        let log = wood_log_id();
        let allows = model(None, json!({"allowed_blocks": [{"id": "block.wood_log"}]}));
        let forbids = model(None, json!({"allowed_blocks": [{"id": "brick"}]}));
        assert_eq!(allows.reconcile(Some(&log)), Some(log.clone()));
        assert_eq!(forbids.reconcile(Some(&log)), None);
        assert_eq!(allows.reconcile(None), None, "nothing is auto-selected");
    }

    #[test]
    fn selection_line_reports_the_selected_name() {
        let m = model(None, json!({"allowed_blocks": [{"id": "block.wood_log"}]}));
        assert_eq!(m.selection_line(None), "Selected: none");
        assert_eq!(m.selection_line(Some(&wood_log_id())), "Selected: Wood Log");
        assert_eq!(
            m.selection_line(Some(&BlockId::new("brick"))),
            "Selected: none"
        );
    }
}
