//! The SFX catalog: stable ids, file names, groups, levels and repetition
//! policy. Add a sound here (and nowhere else) to make it playable.

use std::time::Duration;

use bevy::prelude::Resource;

/// Folder of the sound effects below the assets directory.
pub const SFX_DIR: &str = "audio/sfx";

/// Every sound effect of the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Sfx {
    BlockPlace,
    BlockRemove,
    ShopPurchase,
    ActionDenied,
    MathCorrect,
    MathIncorrect,
    StructureCollapse,
    StageComplete,
}

/// Volume category (a later settings screen can expose one slider per group).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SfxGroup {
    /// Actions in the world: placing and removing blocks, collapse.
    World,
    /// Feedback for interface actions: shop and math answers.
    Ui,
    /// Rare, important moments: stage completion.
    Major,
}

impl Sfx {
    pub const ALL: [Sfx; 8] = [
        Sfx::BlockPlace,
        Sfx::BlockRemove,
        Sfx::ShopPurchase,
        Sfx::ActionDenied,
        Sfx::MathCorrect,
        Sfx::MathIncorrect,
        Sfx::StructureCollapse,
        Sfx::StageComplete,
    ];

    /// File name without extension: lower-case snake_case.
    pub const fn file_stem(self) -> &'static str {
        match self {
            Sfx::BlockPlace => "block_place",
            Sfx::BlockRemove => "block_remove",
            Sfx::ShopPurchase => "shop_purchase",
            Sfx::ActionDenied => "action_denied",
            Sfx::MathCorrect => "math_correct",
            Sfx::MathIncorrect => "math_incorrect",
            Sfx::StructureCollapse => "structure_collapse",
            Sfx::StageComplete => "stage_complete",
        }
    }

    /// Asset path (relative to the assets directory), always an `.ogg`.
    pub fn path(self) -> String {
        format!("{SFX_DIR}/{}.ogg", self.file_stem())
    }

    pub const fn group(self) -> SfxGroup {
        match self {
            Sfx::BlockPlace | Sfx::BlockRemove | Sfx::StructureCollapse => SfxGroup::World,
            Sfx::ShopPurchase | Sfx::ActionDenied | Sfx::MathCorrect | Sfx::MathIncorrect => {
                SfxGroup::Ui
            }
            Sfx::StageComplete => SfxGroup::Major,
        }
    }

    /// Level trim of this sound inside its group (0.0..=1.0). The files are not
    /// assumed to have the same loudness: tune them here, in one place.
    pub const fn trim(self) -> f32 {
        match self {
            Sfx::BlockPlace => 0.8,
            Sfx::BlockRemove => 0.8,
            Sfx::ShopPurchase => 0.8,
            Sfx::ActionDenied => 0.7,
            Sfx::MathCorrect => 0.9,
            Sfx::MathIncorrect => 0.7,
            Sfx::StructureCollapse => 0.9,
            Sfx::StageComplete => 1.0,
        }
    }

    /// Minimum time between two plays of the *same* sound. Zero for ordinary
    /// actions, so rapid placing and removing is never suppressed; a short guard
    /// for sounds that could burst (a denied action held down, several
    /// collapses in a row).
    pub const fn min_interval(self) -> Duration {
        match self {
            Sfx::ActionDenied => Duration::from_millis(150),
            Sfx::StructureCollapse => Duration::from_millis(400),
            _ => Duration::ZERO,
        }
    }

    /// What the sound is for (shown in the missing-file report and the docs).
    pub const fn purpose(self) -> &'static str {
        match self {
            Sfx::BlockPlace => "a block was placed",
            Sfx::BlockRemove => "a block was removed with the hammer",
            Sfx::ShopPurchase => "a purchase succeeded",
            Sfx::ActionDenied => "a purchase was refused",
            Sfx::MathCorrect => "a math answer was correct",
            Sfx::MathIncorrect => "a math answer was wrong",
            Sfx::StructureCollapse => "a structure collapsed",
            Sfx::StageComplete => "the stage was completed",
        }
    }
}

/// Central volume configuration. A later audio-settings milestone only needs to
/// change these numbers (master and per group).
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct SfxVolumes {
    pub master: f32,
    pub world: f32,
    pub ui: f32,
    pub major: f32,
}

impl Default for SfxVolumes {
    fn default() -> Self {
        Self {
            master: 0.8,
            world: 0.7,
            ui: 0.8,
            major: 0.9,
        }
    }
}

impl SfxVolumes {
    fn group(&self, group: SfxGroup) -> f32 {
        match group {
            SfxGroup::World => self.world,
            SfxGroup::Ui => self.ui,
            SfxGroup::Major => self.major,
        }
    }

    /// Linear gain for a sound: master x group x trim, kept within 0..=1 so no
    /// sound can be amplified above its file's own level.
    pub fn gain(&self, sfx: Sfx) -> f32 {
        (self.master * self.group(sfx.group()) * sfx.trim()).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn paths_follow_the_convention() {
        let mut stems = HashSet::new();
        for sfx in Sfx::ALL {
            let path = sfx.path();
            assert_eq!(path, format!("audio/sfx/{}.ogg", sfx.file_stem()));
            assert!(path.ends_with(".ogg"));
            let stem = sfx.file_stem();
            assert!(
                !stem.is_empty()
                    && stem.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                    && !stem.starts_with('_')
                    && !stem.ends_with('_'),
                "{stem} is not lower-case snake_case"
            );
            assert!(stems.insert(stem), "duplicate file name {stem}");
        }
        assert_eq!(stems.len(), Sfx::ALL.len());
    }

    #[test]
    fn the_catalog_lists_every_variant_once() {
        let mut seen = HashSet::new();
        for sfx in Sfx::ALL {
            assert!(seen.insert(sfx));
        }
        // Exhaustive on purpose: adding a variant breaks this match until ALL is updated.
        for sfx in Sfx::ALL {
            match sfx {
                Sfx::BlockPlace | Sfx::BlockRemove | Sfx::ShopPurchase | Sfx::ActionDenied
                | Sfx::MathCorrect | Sfx::MathIncorrect | Sfx::StructureCollapse
                | Sfx::StageComplete => {}
            }
        }
    }

    #[test]
    fn default_levels_are_conservative_and_never_amplify() {
        let volumes = SfxVolumes::default();
        for sfx in Sfx::ALL {
            let gain = volumes.gain(sfx);
            assert!((0.2..=0.9).contains(&gain), "{sfx:?} gain {gain}");
            assert!((0.0..=1.0).contains(&sfx.trim()));
        }
        let loud = SfxVolumes { master: 5.0, world: 5.0, ui: 5.0, major: 5.0 };
        assert!(Sfx::ALL.iter().all(|s| loud.gain(*s) <= 1.0));
        let muted = SfxVolumes { master: 0.0, ..SfxVolumes::default() };
        assert!(Sfx::ALL.iter().all(|s| muted.gain(*s) == 0.0));
    }

    #[test]
    fn ordinary_actions_have_no_cooldown_but_bursty_sounds_do() {
        assert_eq!(Sfx::BlockPlace.min_interval(), Duration::ZERO);
        assert_eq!(Sfx::BlockRemove.min_interval(), Duration::ZERO);
        assert!(Sfx::StructureCollapse.min_interval() > Duration::ZERO);
        assert!(Sfx::ActionDenied.min_interval() > Duration::ZERO);
    }

    #[test]
    fn major_events_are_in_their_own_group() {
        assert_eq!(Sfx::StageComplete.group(), SfxGroup::Major);
        assert_eq!(Sfx::BlockPlace.group(), SfxGroup::World);
        assert_eq!(Sfx::MathCorrect.group(), SfxGroup::Ui);
    }
}
