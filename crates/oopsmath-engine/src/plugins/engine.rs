use bevy::prelude::*;

use crate::app::{
    boot::boot_system,
    states::AppState,
};

pub struct OopsMathEnginePlugin;

impl Plugin for OopsMathEnginePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>()
            .add_systems(OnEnter(AppState::Boot), boot_system);
    }
}