//! End-to-end checks of the localization data: a pack built by
//! `tools/oopsmath-i18n` (a real zip), and the language folder shipped in
//! `assets/i18n`.

use std::path::Path;

use bevy::prelude::IVec3;
use oopsmath_engine::blocks::{BlockId, PlacementRejection};
use oopsmath_engine::blueprint::PlanIssue;
use oopsmath_engine::challenges::{AnswerError, SubmitError};
use oopsmath_engine::i18n::catalog::LanguageCatalog;
use oopsmath_engine::i18n::meta::Direction;
use oopsmath_engine::i18n::pack::{PackFiles, read_i18n_dir};
use oopsmath_engine::i18n::service::I18n;
use oopsmath_engine::i18n::translator::scan_messages;
use oopsmath_engine::items::shop::PurchaseError;

const FA_ZIP: &[u8] = include_bytes!("fixtures/i18n_pack_fa.zip");

fn shipped_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/i18n")
}

#[test]
fn a_pack_built_by_the_tool_is_readable() {
    let pack = PackFiles::from_zip_bytes("fa.zip", FA_ZIP).expect("valid zip");
    let keys: Vec<&str> = pack.files.keys().map(String::as_str).collect();
    assert_eq!(keys, ["fa/_meta.ron", "fa/core.ftl", "fa/levels/001_first_wall.ftl"]);

    let catalog = LanguageCatalog::from_packs(&[pack]);
    assert!(catalog.diagnostics.is_empty(), "{:?}", catalog.diagnostics);
    let fa = catalog.language("fa").expect("fa installed");
    assert_eq!(fa.meta.direction, Direction::Rtl);
    assert!(fa.levels.contains_key("001_first_wall"));
    assert!(catalog.level_overlay().contains_key("fa"));

    // A pack without English still falls back to the key, never to nothing.
    let i18n = I18n::new(catalog, "fa");
    assert_eq!(i18n.active_code(), "fa");
    assert_eq!(i18n.tr("session-resume"), "ادامه");
    assert_eq!(i18n.tr("no-such-message"), "no-such-message");
}

#[test]
fn the_shipped_languages_are_complete_and_valid() {
    let (packs, read_problems) = read_i18n_dir(&shipped_dir());
    assert!(read_problems.is_empty(), "{read_problems:?}");
    let catalog = LanguageCatalog::from_packs(&packs);
    assert!(catalog.diagnostics.is_empty(), "{:?}", catalog.diagnostics);
    let english = catalog.language("en-US").expect("en-US is shipped");
    let ids: Vec<String> = english
        .core
        .iter()
        .flat_map(|source| scan_messages(source))
        .map(|(id, _)| id)
        .collect();
    assert!(ids.len() > 40, "the English source has the core strings");

    let en = I18n::new(catalog.clone(), "en-US");
    for id in &ids {
        assert_ne!(&en.tr(id), id, "English message '{id}' does not resolve");
    }
    assert!(en.diagnostics().next().is_none(), "{:?}", en.diagnostics().collect::<Vec<_>>());

    // Every other shipped language only uses messages that exist in English
    // (a stale key is a typo or a removed string) and has no syntax errors.
    for code in catalog.codes().into_iter().filter(|c| *c != "en-US") {
        let i18n = I18n::new(catalog.clone(), code);
        assert_eq!(i18n.active_code(), code);
        assert!(i18n.diagnostics().next().is_none(), "{code}: {:?}", i18n.diagnostics().collect::<Vec<_>>());
        let data = catalog.language(code).unwrap();
        for source in &data.core {
            for (id, _) in scan_messages(source) {
                assert!(ids.contains(&id), "{code} defines '{id}', which English does not");
            }
        }
    }
}

#[test]
fn the_example_stage_has_translated_level_text() {
    let (packs, _) = read_i18n_dir(&shipped_dir());
    let overlay = LanguageCatalog::from_packs(&packs).level_overlay();
    let en = &overlay["en-US"];
    let fa = &overlay["fa"];
    assert!(en.get("stage.001_first_wall.title").is_some());
    assert!(fa.get("stage.001_first_wall.title").is_some());
}

fn shipped(language: &str) -> I18n {
    let (packs, _) = read_i18n_dir(&shipped_dir());
    I18n::new(LanguageCatalog::from_packs(&packs), language)
}

/// The text the game builds in code is matched against English templates in
/// `core.ftl` (see `docs/i18n/README.md`). Building the real strings here means
/// a change to a message in the code breaks this test instead of silently
/// leaving that text untranslated.
#[test]
fn text_the_game_builds_is_recognised_and_translated() {
    let fa = shipped("fa");
    let id = BlockId::new("block.wood_log");
    let cell = IVec3::new(1, 0, 2);
    let texts: Vec<String> = vec![
        PlacementRejection::NothingSelected.to_string(),
        PlacementRejection::NotAllowed(id.clone()).to_string(),
        PlacementRejection::NotRegistered(id.clone()).to_string(),
        PlacementRejection::OutOfBounds { block: id.clone(), cell }.to_string(),
        PlacementRejection::Occupied { block: id.clone(), cell }.to_string(),
        PlacementRejection::Unsupported { block: id.clone() }.to_string(),
        PlacementRejection::NoInventory(id.clone()).to_string(),
        PlacementRejection::LimitReached { block: id.clone(), max: 5 }.to_string(),
        PurchaseError::ZeroQuantity.to_string(),
        PurchaseError::UnknownBlock(id.clone()).to_string(),
        PurchaseError::NotAllowed(id.clone()).to_string(),
        PurchaseError::NoPrice(id.clone()).to_string(),
        PurchaseError::PriceOverflow.to_string(),
        PurchaseError::InsufficientFunds { needed: 9, balance: 3 }.to_string(),
        PurchaseError::InventoryFull.to_string(),
        PlanIssue::Unregistered(id.clone()).to_string(),
        PlanIssue::NotAllowed(id.clone()).to_string(),
        PlanIssue::ExceedsStageLimit { block: id.clone(), required: 8, max: 5 }.to_string(),
        PlanIssue::NoPrice(id.clone()).to_string(),
        PlanIssue::CostOverflow.to_string(),
        AnswerError::Empty.to_string(),
        AnswerError::Malformed.to_string(),
        AnswerError::OutOfRange.to_string(),
        SubmitError::UnknownChallenge.to_string(),
        format!("Cannot build: {}", PlanIssue::NotAllowed(id.clone())),
        format!("Could not place Wood Log: {}", PlacementRejection::Occupied { block: id.clone(), cell }),
    ];
    for text in texts {
        let translated = fa
            .translate_text(&text)
            .unwrap_or_else(|| panic!("not recognised as a message: {text}"));
        assert_ne!(translated, text, "not translated: {text}");
    }
}

#[test]
fn block_and_tool_names_inside_text_are_translated() {
    let fa = shipped("fa");
    let en = shipped("en-US");
    // (text as the game builds it, part of the Persian result, the English result)
    for (built, persian_part, english_result) in [
        ("You have no Wood Log in your inventory.", "کنده چوب", "You have no Wood Log in your inventory."),
        ("Removed Wood Log; 1 returned to your inventory.", "کنده چوب", "Removed Wood Log; 1 returned to your inventory."),
        ("> [Tool] Hammer x1", "چکش", "> [Tool] Hammer x1"),
        ("Selected: Hammer (Tool)", "چکش", "Selected: Hammer (Tool)"),
        ("Placed Wood Log at (1, 0, -2)", "کنده چوب", "Placed Wood Log at (1, 0, -2)"),
        (
            "Wood Log: need 8, have 2, 6 more needed (3 coins each = 9 coins)",
            "کنده چوب",
            "Wood Log: need 8, have 2, 6 more needed (3 coins each = 9 coins)",
        ),
        // A raw block id shown to the player becomes the block's name in every
        // language, English included.
        (
            "You have no block.wood_log in your inventory.",
            "کنده چوب",
            "You have no Wood Log in your inventory.",
        ),
    ] {
        let translated = fa.translate_text(built).unwrap_or_else(|| panic!("{built}"));
        assert!(translated.contains(persian_part), "{built} -> {translated}");
        assert_eq!(
            en.translate_text(built).unwrap_or_else(|| built.to_string()),
            english_result,
            "English output for: {built}"
        );
    }
}

#[test]
fn the_answer_label_and_the_other_dynamic_lines_follow_the_language() {
    let fa = shipped("fa");
    assert_eq!(fa.translate_text("Answer: 12_").as_deref(), Some("پاسخ: 12_"));
    assert_eq!(fa.translate_text("Answer: _").as_deref(), Some("پاسخ: _"));
    assert_eq!(fa.translate_text("Challenge 1 / 2").as_deref(), Some("چالش 1 از 2"));
    assert_eq!(fa.translate_text("Money: 25 coins").as_deref(), Some("پول: 25 سکه"));
    assert_eq!(
        fa.translate_text("In inventory: 8\nPlaced: 3 / 20").as_deref(),
        Some("در انبار: 8\nگذاشته‌شده: 3 از 20")
    );
    assert_eq!(
        fa.translate_text("Plan: 3 / 8 built").as_deref(),
        Some("نقشه: 3 از 8 ساخته شد")
    );
    assert_eq!(fa.translate_text("Level: 1").as_deref(), Some("سطح: 1"));
}

#[test]
fn switching_back_to_english_restores_the_text() {
    let mut i18n = shipped("fa");
    let english = "Money: 25 coins";
    let persian = i18n.translate_text(english).unwrap();
    assert_ne!(persian, english);
    assert!(i18n.set_language("en-US"));
    assert_eq!(i18n.translate_text(english).as_deref(), Some(english));
    assert!(i18n.set_language("fa"));
    assert_eq!(i18n.translate_text(english).as_deref(), Some(persian.as_str()));
}
