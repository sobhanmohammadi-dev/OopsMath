use bevy::prelude::*;

#[derive(Component)]
pub struct GameplayHud;

#[derive(Component)]
pub struct MoneyText;

#[derive(Component)]
pub struct MaterialText;

#[derive(Component)]
pub struct ToolText;

#[derive(Component)]
pub struct ObjectiveText;

#[derive(Component)]
pub struct UiButton {
    pub action: UiAction,
}

#[derive(Clone, Copy, Debug)]
pub enum UiAction {
    Play,
}