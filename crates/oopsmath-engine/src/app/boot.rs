use bevy::prelude::*;

use super::states::AppState;

pub fn boot_system(mut next_state: ResMut<NextState<AppState>>) {
    next_state.set(AppState::MainMenu);
}