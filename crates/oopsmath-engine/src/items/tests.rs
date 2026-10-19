use serde_json::json;

use super::ItemRef;
use super::inventory::{Inventory, InventoryError, ResourceError, StartingResources};
use super::tools::{
    ToolDefinition, ToolError, ToolId, ToolKind, ToolRegistry, hammer_id, parse_allowed_tools,
    register_builtin_tools,
};
use crate::blocks::models::{register_builtin_blocks, wood_log_id};
use crate::blocks::{BlockId, BlockRegistry};
use crate::stage::localization::{StageLocale, StageMessages};
use crate::ui::hud::hotbar::{
    HotbarStatus, build_hotbar, is_item_selectable, reconcile_item, selection_line,
};
use crate::ui::hud::model::StageHudModel;

fn blocks() -> BlockRegistry {
    let mut r = BlockRegistry::new();
    register_builtin_blocks(&mut r).unwrap();
    r
}

fn tools() -> ToolRegistry {
    let mut r = ToolRegistry::default();
    register_builtin_tools(&mut r).unwrap();
    r
}

fn log() -> ItemRef {
    ItemRef::Block(wood_log_id())
}

fn hammer() -> ItemRef {
    ItemRef::Tool(hammer_id())
}

/// Stage HUD model for a construction section.
fn model(construction: serde_json::Value) -> StageHudModel {
    StageHudModel::from_parts(
        "stage.t.title",
        None,
        &StageMessages::from_localizations(&[]),
        &construction,
        &blocks(),
        &StageLocale::from("en-US"),
    )
}

fn stage_with_hammer_and_log() -> serde_json::Value {
    json!({
        "allowed_blocks": [{"id": "block.wood_log"}],
        "allowed_tools": [{"id": "hammer"}],
        "starting_inventory": {"blocks": {"block.wood_log": 5}, "tools": {"hammer": 1}}
    })
}

// ----------------------------------------------------------- inventory

#[test]
fn add_query_and_has() {
    let mut inv = Inventory::default();
    assert_eq!(inv.quantity(&log()), 0);
    assert_eq!(inv.add(log(), 3), Ok(3));
    assert_eq!(inv.add(log(), 2), Ok(5));
    assert_eq!(inv.quantity(&log()), 5);
    assert!(inv.has(&log(), 5) && !inv.has(&log(), 6));
    assert!(!inv.has(&hammer(), 1));
}

#[test]
fn invalid_quantities_and_overflow_are_rejected_without_change() {
    let mut inv = Inventory::default();
    assert_eq!(inv.add(log(), 0), Err(InventoryError::ZeroQuantity));
    inv.add(log(), u32::MAX).unwrap();
    assert_eq!(inv.add(log(), 1), Err(InventoryError::Overflow(log())));
    assert_eq!(inv.quantity(&log()), u32::MAX);
    assert_eq!(inv.remove(&log(), 0), Err(InventoryError::ZeroQuantity));
    let mut small = Inventory::default();
    small.add(hammer(), 1).unwrap();
    assert_eq!(
        small.remove(&hammer(), 2),
        Err(InventoryError::Insufficient {
            item: hammer(),
            have: 1,
            requested: 2
        })
    );
    assert_eq!(small.quantity(&hammer()), 1);
}

#[test]
fn reaching_zero_removes_the_entry() {
    let mut inv = Inventory::default();
    inv.add(hammer(), 2).unwrap();
    assert_eq!(inv.remove(&hammer(), 1), Ok(1));
    assert_eq!(inv.remove(&hammer(), 1), Ok(0));
    assert!(inv.is_empty());
    assert_eq!(inv.iter().count(), 0);
}

#[test]
fn iteration_is_sorted_and_deterministic() {
    let mut inv = Inventory::default();
    inv.add(hammer(), 1).unwrap();
    inv.add(log(), 4).unwrap();
    let order: Vec<_> = inv.iter().map(|(i, q)| (i.clone(), q)).collect();
    assert_eq!(
        order,
        [(log(), 4), (hammer(), 1)],
        "blocks sort before tools"
    );
}

// ------------------------------------------------- starting resources

#[test]
fn starting_resources_initialise_the_inventory() {
    let starting = StartingResources::parse(
        &json!({"starting_inventory": {"blocks": {"block.wood_log": 5, "brick": 0}, "tools": {"hammer": 1}}}),
        Some(&json!({"budget": 100})),
    )
    .unwrap();
    assert_eq!(starting.currency, Some(100));
    assert_eq!(
        starting.items,
        [(log(), 5), (hammer(), 1)],
        "zero entries are ignored"
    );
    let inv = Inventory::from_starting(&starting).unwrap();
    assert_eq!(inv.quantity(&log()), 5);
    assert_eq!(inv.quantity(&hammer()), 1);
}

#[test]
fn legacy_stages_get_nothing() {
    for construction in [json!({}), json!({"starting_inventory": null})] {
        let starting = StartingResources::parse(&construction, None).unwrap();
        assert_eq!(starting, StartingResources::default());
        assert!(Inventory::from_starting(&starting).unwrap().is_empty());
    }
    // A budget alone grants currency data but no items.
    let s = StartingResources::parse(&json!({}), Some(&json!({"budget": 7}))).unwrap();
    assert!(s.items.is_empty() && s.currency == Some(7));
}

#[test]
fn malformed_starting_resources_are_errors() {
    for bad in [
        json!({"starting_inventory": {"tools": {"hammer": -1}}}),
        json!({"starting_inventory": {"tools": {"hammer": "one"}}}),
        json!({"starting_inventory": {"blocks": {"b": 4294967296u64}}}),
        json!({"starting_inventory": {"tools": [1]}}),
        json!({"starting_inventory": 5}),
    ] {
        assert!(StartingResources::parse(&bad, None).is_err(), "{bad}");
    }
    for budget in [json!(-1), json!(1.5), json!("x"), json!(1e12)] {
        assert_eq!(
            StartingResources::parse(&json!({}), Some(&json!({"budget": budget}))),
            Err(ResourceError::InvalidBudget)
        );
    }
}

#[test]
fn restarting_a_stage_rebuilds_the_inventory_deterministically() {
    let a = StartingResources::parse(&stage_with_hammer_and_log(), None).unwrap();
    let first = Inventory::from_starting(&a).unwrap();
    let second = Inventory::from_starting(&a).unwrap();
    assert_eq!(first, second);
    let b = StartingResources::parse(
        &json!({"starting_inventory": {"tools": {"hammer": 2}}}),
        None,
    )
    .unwrap();
    let other = Inventory::from_starting(&b).unwrap();
    assert_eq!(other.quantity(&hammer()), 2);
    assert_eq!(
        other.quantity(&log()),
        0,
        "nothing carries over from the previous stage"
    );
}

// ------------------------------------------------------ classification

#[test]
fn tools_never_resolve_as_blocks() {
    let registry = blocks();
    assert!(hammer().resolve_block(&registry).is_none());
    // Even a tool whose id equals a block id stays a tool.
    assert!(
        ItemRef::Tool(ToolId::new("block.wood_log"))
            .resolve_block(&registry)
            .is_none()
    );
    assert!(hammer().as_block().is_none() && log().as_tool().is_none());
}

#[test]
fn block_items_resolve_to_registry_definitions() {
    let registry = blocks();
    let def = log().resolve_block(&registry).unwrap();
    assert!(std::ptr::eq(def, registry.get(&wood_log_id()).unwrap()));
    assert!(
        ItemRef::Block(BlockId::new("brick"))
            .resolve_block(&registry)
            .is_none()
    );
}

#[test]
fn tool_registry_validates_ids_and_duplicates() {
    let mut r = tools();
    assert_eq!(r.get(&hammer_id()).unwrap().kind, ToolKind::Hammer);
    assert_eq!(
        r.register(ToolDefinition::new(hammer_id(), "Again", ToolKind::Hammer)),
        Err(ToolError::Duplicate(hammer_id()))
    );
    for bad in ["", "Hammer", "a..b", "1x", "has space"] {
        assert!(
            matches!(
                r.register(ToolDefinition::new(ToolId::new(bad), "X", ToolKind::Hammer)),
                Err(ToolError::InvalidId(_))
            ),
            "{bad}"
        );
    }
    assert!(matches!(
        r.register(ToolDefinition::new(
            ToolId::new("saw"),
            "  ",
            ToolKind::Hammer
        )),
        Err(ToolError::EmptyName(_))
    ));
}

#[test]
fn allowed_tools_parse() {
    assert_eq!(
        parse_allowed_tools(&json!({"allowed_tools": [{"id": "hammer"}]})).unwrap(),
        [hammer_id()]
    );
    assert!(parse_allowed_tools(&json!({})).unwrap().is_empty());
    assert!(parse_allowed_tools(&json!({"allowed_tools": [{}]})).is_err());
}

// ------------------------------------------------ selection and hotbar

#[test]
fn available_items_are_selectable() {
    let m = model(stage_with_hammer_and_log());
    let inv = Inventory::from_starting(
        &StartingResources::parse(&stage_with_hammer_and_log(), None).unwrap(),
    )
    .unwrap();
    assert!(is_item_selectable(&hammer(), &m, &inv, &tools()));
    assert!(is_item_selectable(&log(), &m, &inv, &tools()));
}

#[test]
fn absent_or_stage_disallowed_items_are_rejected() {
    let m = model(stage_with_hammer_and_log());
    let empty = Inventory::default();
    // Tool not in the inventory.
    assert!(!is_item_selectable(&hammer(), &m, &empty, &tools()));
    // Tool in the inventory but the stage does not allow it.
    let mut inv = Inventory::default();
    inv.add(hammer(), 1).unwrap();
    let no_tools = model(json!({"allowed_blocks": [{"id": "block.wood_log"}]}));
    assert!(!is_item_selectable(&hammer(), &no_tools, &inv, &tools()));
    // Unregistered tool and unregistered/disallowed block.
    let ghost = ItemRef::Tool(ToolId::new("laser"));
    inv.add(ghost.clone(), 1).unwrap();
    let allows_ghost = model(json!({"allowed_tools": [{"id": "laser"}]}));
    assert!(!is_item_selectable(&ghost, &allows_ghost, &inv, &tools()));
    assert!(!is_item_selectable(
        &ItemRef::Block(BlockId::new("brick")),
        &m,
        &inv,
        &tools()
    ));
    assert!(!is_item_selectable(
        &log(),
        &no_tools_blocks(),
        &inv,
        &tools()
    ));
}

fn no_tools_blocks() -> StageHudModel {
    model(json!({"allowed_blocks": []}))
}

#[test]
fn selected_tool_is_cleared_when_its_quantity_reaches_zero() {
    let m = model(stage_with_hammer_and_log());
    let mut inv = Inventory::default();
    inv.add(hammer(), 1).unwrap();
    let selected = Some(hammer());
    assert_eq!(
        reconcile_item(selected.as_ref(), &m, &inv, &tools()),
        selected
    );
    inv.remove(&hammer(), 1).unwrap();
    assert_eq!(reconcile_item(selected.as_ref(), &m, &inv, &tools()), None);
}

#[test]
fn stage_change_clears_a_selection_the_new_stage_forbids() {
    let old = model(stage_with_hammer_and_log());
    let new = model(json!({"allowed_blocks": [{"id": "brick"}]}));
    let mut inv = Inventory::default();
    inv.add(hammer(), 1).unwrap();
    assert_eq!(
        reconcile_item(Some(&hammer()), &old, &inv, &tools()),
        Some(hammer())
    );
    assert_eq!(reconcile_item(Some(&hammer()), &new, &inv, &tools()), None);
    assert_eq!(reconcile_item(Some(&log()), &new, &inv, &tools()), None);
    assert_eq!(
        reconcile_item(None, &old, &inv, &tools()),
        None,
        "nothing is auto-selected"
    );
}

#[test]
fn blocks_need_inventory_to_be_selectable() {
    let m = model(json!({"allowed_blocks": [{"id": "block.wood_log"}]}));
    assert!(!is_item_selectable(
        &log(),
        &m,
        &Inventory::default(),
        &tools()
    ));
    let mut inv = Inventory::default();
    inv.add(log(), 1).unwrap();
    assert!(is_item_selectable(&log(), &m, &inv, &tools()));
    // Using the last unit clears the selection.
    inv.remove(&log(), 1).unwrap();
    assert_eq!(reconcile_item(Some(&log()), &m, &inv, &tools()), None);
}

#[test]
fn hotbar_lists_inventory_with_status_and_labels() {
    let m = model(json!({
        "allowed_blocks": [{"id": "block.wood_log"}],
        "allowed_tools": [{"id": "hammer"}]
    }));
    let mut inv = Inventory::default();
    inv.add(log(), 5).unwrap();
    inv.add(hammer(), 1).unwrap();
    inv.add(ItemRef::Block(BlockId::new("brick")), 2).unwrap();
    inv.add(ItemRef::Tool(ToolId::new("laser")), 1).unwrap();
    let bar = build_hotbar(&inv, &m, &blocks(), &tools());
    let labels: Vec<_> = bar.iter().map(|e| e.label(false)).collect();
    // Sorted: block.wood_log, brick, hammer, laser.
    assert_eq!(labels[0], "[Block] Wood Log x5");
    assert_eq!(
        labels[1],
        "[Block] brick x2 - unavailable: not a registered block"
    );
    assert_eq!(labels[2], "[Tool] Hammer x1");
    assert_eq!(
        labels[3],
        "[Tool] laser x1 - unavailable: not a registered tool"
    );
    assert_eq!(bar[0].label(true), "> [Block] Wood Log x5");
    assert_eq!(bar[1].status, HotbarStatus::NotRegistered);
    // Registered but not allowed by the stage.
    let strict = model(json!({}));
    let bar = build_hotbar(&inv, &strict, &blocks(), &tools());
    assert_eq!(bar[0].status, HotbarStatus::NotAllowed);
    assert!(bar[0].label(false).ends_with("not allowed in this stage"));
}

#[test]
fn selection_line_distinguishes_kinds() {
    assert_eq!(selection_line(None, &blocks(), &tools()), "Selected: none");
    assert_eq!(
        selection_line(Some(&log()), &blocks(), &tools()),
        "Selected: Wood Log (Block)"
    );
    assert_eq!(
        selection_line(Some(&hammer()), &blocks(), &tools()),
        "Selected: Hammer (Tool)"
    );
}
