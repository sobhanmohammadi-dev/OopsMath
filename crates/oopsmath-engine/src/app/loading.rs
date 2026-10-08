use bevy::prelude::*;

use super::states::AppState;

pub fn load_test_stage(
    mut next_state: ResMut<NextState<AppState>>,
) {
    next_state.set(AppState::InGame);
}