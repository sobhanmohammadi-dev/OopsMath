use bevy::prelude::*;

use super::states::AppState;

pub fn boot_system(mut commands: Commands, mut next_state: ResMut<NextState<AppState>>) {
    // Camera used for both the UI and future 2D rendering.
    commands.spawn(Camera2d);

    next_state.set(AppState::MainMenu);
}
