//! Turns changes of the authoritative game state into sound requests.
//!
//! The game already keeps its committed state in resources. A sound is requested
//! when that state *changes in a way only a real action can cause*, so it cannot
//! be triggered by previews, redraws or conditions that stay true:
//!
//! | Sound | Edge |
//! |---|---|
//! | block placed | the number of placed blocks went up |
//! | block removed | the number of placed blocks went down (collapse only moves blocks) |
//! | purchase | the wallet balance went down (only the shop spends money) |
//! | purchase refused | the shop result message was set while nothing else about the shop changed, and no money was spent |
//! | math correct | the number of solved challenges went up |
//! | math wrong | the challenge feedback was set to the "not correct" message while nothing else about the panel changed |
//! | collapse | the number of falling blocks went up (a new collapse started) |
//! | stage complete | the objectives became complete (false -> true) |
//!
//! The first observation of a stage run only records a baseline, so entering a
//! stage, restarting it or rebuilding its state never plays a sound.
//!
//! "Nothing else changed" is checked by comparing the new state with the
//! previous one after replacing only the message, so opening the Shop, changing
//! the quantity, typing an answer or closing a panel never counts as an attempt.

use crate::challenges::ChallengePanel;
use crate::items::shop::ShopState;

use super::catalog::Sfx;

/// The feedback `ChallengePanel::submit` sets for a wrong answer. A test in
/// `tests/audio_contract.rs` runs the real submit and checks this text.
pub const INCORRECT_FEEDBACK: &str = "Not correct. Try again.";

/// What the game looks like right now.
pub struct Observed<'a> {
    /// Placed blocks (not cells).
    pub placed: usize,
    pub balance: u32,
    pub shop: &'a ShopState,
    /// The shop resource was written to since the last observation.
    pub shop_changed: bool,
    pub panel: &'a ChallengePanel,
    pub panel_changed: bool,
    pub solved: usize,
    pub falling: usize,
    pub complete: bool,
}

#[derive(Clone)]
struct Previous {
    placed: usize,
    balance: u32,
    shop: ShopState,
    panel: ChallengePanel,
    solved: usize,
    falling: usize,
    complete: bool,
}

impl Previous {
    fn of(o: &Observed) -> Self {
        Self {
            placed: o.placed,
            balance: o.balance,
            shop: o.shop.clone(),
            panel: o.panel.clone(),
            solved: o.solved,
            falling: o.falling,
            complete: o.complete,
        }
    }
}

/// Edge detector. One per stage run: call [`SfxEdges::reset`] when a run starts.
#[derive(bevy::prelude::Resource, Default)]
pub struct SfxEdges {
    previous: Option<Previous>,
}

fn refused_purchase(before: &ShopState, now: &ShopState) -> bool {
    if now.message.is_none() {
        return false;
    }
    let mut probe = before.clone();
    probe.message = now.message.clone();
    probe == *now
}

fn wrong_answer(before: &ChallengePanel, now: &ChallengePanel) -> bool {
    if now.feedback.as_deref() != Some(INCORRECT_FEEDBACK) {
        return false;
    }
    let mut probe = before.clone();
    probe.feedback = now.feedback.clone();
    probe == *now
}

impl SfxEdges {
    /// Forget the previous state (a new stage run starts).
    pub fn reset(&mut self) {
        self.previous = None;
    }

    /// Compares the state with the previous observation and returns the sounds
    /// to play, at most once each.
    pub fn observe(&mut self, now: &Observed) -> Vec<Sfx> {
        let Some(before) = self.previous.take() else {
            self.previous = Some(Previous::of(now));
            return Vec::new();
        };
        let mut sounds = Vec::new();

        if now.placed > before.placed {
            sounds.push(Sfx::BlockPlace);
        } else if now.placed < before.placed {
            sounds.push(Sfx::BlockRemove);
        }

        if now.balance < before.balance {
            sounds.push(Sfx::ShopPurchase);
        } else if now.shop_changed && refused_purchase(&before.shop, now.shop) {
            sounds.push(Sfx::ActionDenied);
        }

        if now.solved > before.solved {
            sounds.push(Sfx::MathCorrect);
        } else if now.panel_changed && wrong_answer(&before.panel, now.panel) {
            sounds.push(Sfx::MathIncorrect);
        }

        if now.falling > before.falling {
            sounds.push(Sfx::StructureCollapse);
        }

        if now.complete && !before.complete {
            // The completion fanfare replaces the "correct" sound of the answer
            // that finished the stage, so the two never stack.
            sounds.retain(|s| *s != Sfx::MathCorrect);
            sounds.push(Sfx::StageComplete);
        }

        self.previous = Some(Previous::of(now));
        sounds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct World {
        placed: usize,
        balance: u32,
        shop: ShopState,
        panel: ChallengePanel,
        solved: usize,
        falling: usize,
        complete: bool,
    }

    impl World {
        fn new() -> Self {
            Self {
                placed: 0,
                balance: 10,
                shop: ShopState::default(),
                panel: ChallengePanel::default(),
                solved: 0,
                falling: 0,
                complete: false,
            }
        }

        fn observe(&self, edges: &mut SfxEdges, shop_changed: bool, panel_changed: bool) -> Vec<Sfx> {
            edges.observe(&Observed {
                placed: self.placed,
                balance: self.balance,
                shop: &self.shop,
                shop_changed,
                panel: &self.panel,
                panel_changed,
                solved: self.solved,
                falling: self.falling,
                complete: self.complete,
            })
        }
    }

    fn started() -> (SfxEdges, World) {
        let mut edges = SfxEdges::default();
        let world = World::new();
        assert!(world.observe(&mut edges, true, true).is_empty(), "the first observation is a baseline");
        (edges, world)
    }

    #[test]
    fn nothing_changing_plays_nothing_however_often_it_is_observed() {
        let (mut edges, world) = started();
        for _ in 0..100 {
            assert!(world.observe(&mut edges, false, false).is_empty());
        }
        // Even a write that changes no value is silent.
        assert!(world.observe(&mut edges, true, true).is_empty());
    }

    #[test]
    fn a_stage_run_starts_silently_even_with_existing_state() {
        let mut edges = SfxEdges::default();
        let mut world = World::new();
        world.placed = 7;
        world.balance = 3;
        world.complete = true;
        assert!(world.observe(&mut edges, true, true).is_empty());
        world.placed = 0; // the stage is restarted
        edges.reset();
        assert!(world.observe(&mut edges, true, true).is_empty(), "no 'removed' sound for the reset");
    }

    #[test]
    fn placing_and_removing_blocks_play_once_per_action_with_no_cooldown() {
        let (mut edges, mut world) = started();
        let mut played = Vec::new();
        for _ in 0..5 {
            world.placed += 1;
            played.extend(world.observe(&mut edges, false, false));
        }
        assert_eq!(played, vec![Sfx::BlockPlace; 5], "rapid placements are not suppressed");
        world.placed -= 1;
        assert_eq!(world.observe(&mut edges, false, false), [Sfx::BlockRemove]);
        assert!(world.observe(&mut edges, false, false).is_empty());
    }

    #[test]
    fn a_purchase_plays_when_money_is_spent_and_not_the_denied_sound() {
        let (mut edges, mut world) = started();
        world.balance = 7;
        world.shop.message = Some("Bought 1 x Wood Log for 3 coins.".into());
        assert_eq!(world.observe(&mut edges, true, false), [Sfx::ShopPurchase]);
        assert!(world.observe(&mut edges, false, false).is_empty(), "no replay while the message stays");
    }

    #[test]
    fn a_refused_purchase_plays_the_denied_sound_for_every_attempt() {
        let (mut edges, mut world) = started();
        world.shop.message = Some("Not enough money: you need 9 coins but have 3.".into());
        assert_eq!(world.observe(&mut edges, true, false), [Sfx::ActionDenied]);
        // The same refusal again (a second click) is a new attempt.
        assert_eq!(world.observe(&mut edges, true, false), [Sfx::ActionDenied]);
        // The popup being redrawn does not write to the shop.
        assert!(world.observe(&mut edges, false, false).is_empty());
    }

    #[test]
    fn opening_the_shop_or_changing_the_quantity_is_not_a_refused_purchase() {
        let (mut edges, mut world) = started();
        world.shop.message = Some("Not enough money: you need 9 coins but have 3.".into());
        world.observe(&mut edges, true, false);
        world.shop.open = true; // the popup opens; an old message is still there
        assert!(world.observe(&mut edges, true, false).is_empty());
        world.shop.open = false;
        assert!(world.observe(&mut edges, true, false).is_empty());
        world.shop.message = None;
        assert!(world.observe(&mut edges, true, false).is_empty(), "clearing the message is silent");
    }

    #[test]
    fn a_wrong_answer_plays_once_per_submission() {
        let (mut edges, mut world) = started();
        world.panel.open = true;
        world.panel.input = "13".into();
        assert!(world.observe(&mut edges, false, true).is_empty(), "opening and typing are silent");
        world.panel.feedback = Some(INCORRECT_FEEDBACK.into());
        assert_eq!(world.observe(&mut edges, false, true), [Sfx::MathIncorrect]);
        assert!(world.observe(&mut edges, false, false).is_empty(), "no replay while the result stays visible");
        // Submitting the same wrong answer again is a new submission.
        assert_eq!(world.observe(&mut edges, false, true), [Sfx::MathIncorrect]);
        // Typing after the result does not replay it.
        world.panel.input = "131".into();
        assert!(world.observe(&mut edges, false, true).is_empty());
    }

    #[test]
    fn other_answer_results_are_silent_and_a_correct_answer_plays_the_correct_sound() {
        let (mut edges, mut world) = started();
        world.panel.open = true;
        world.panel.feedback = Some("Type an answer first.".into());
        assert!(world.observe(&mut edges, false, true).is_empty());
        world.panel.feedback = Some("Already solved.".into());
        assert!(world.observe(&mut edges, false, true).is_empty());
        world.solved = 1;
        world.panel.input.clear();
        world.panel.feedback = Some("Correct! Challenge solved.".into());
        assert_eq!(world.observe(&mut edges, false, true), [Sfx::MathCorrect]);
        assert!(world.observe(&mut edges, false, true).is_empty(), "the correct sound is tied to the solve, not the panel");
    }

    #[test]
    fn a_collapse_plays_once_for_its_blocks_and_again_for_a_new_collapse() {
        let (mut edges, mut world) = started();
        world.falling = 4; // one collapse, four blocks at once
        assert_eq!(world.observe(&mut edges, false, false), [Sfx::StructureCollapse]);
        assert!(world.observe(&mut edges, false, false).is_empty(), "no replay while they fall");
        world.falling = 2; // some have landed
        assert!(world.observe(&mut edges, false, false).is_empty());
        world.falling = 5; // a second collapse starts during the animation
        assert_eq!(world.observe(&mut edges, false, false), [Sfx::StructureCollapse]);
        world.falling = 0;
        assert!(world.observe(&mut edges, false, false).is_empty());
    }

    #[test]
    fn stage_completion_plays_once_and_replaces_the_last_correct_answer_sound() {
        let (mut edges, mut world) = started();
        world.solved = 1; // the answer that completes the stage
        world.complete = true;
        assert_eq!(world.observe(&mut edges, false, true), [Sfx::StageComplete]);
        for _ in 0..10 {
            assert!(world.observe(&mut edges, false, false).is_empty());
        }
    }

    #[test]
    fn a_removal_followed_by_a_collapse_gives_both_sounds_in_their_own_frames() {
        let (mut edges, mut world) = started();
        world.placed = 3;
        edges.observe(&Observed {
            placed: world.placed, balance: world.balance, shop: &world.shop, shop_changed: false,
            panel: &world.panel, panel_changed: false, solved: 0, falling: 0, complete: false,
        });
        world.placed = 2;
        assert_eq!(world.observe(&mut edges, false, false), [Sfx::BlockRemove]);
        world.falling = 1;
        assert_eq!(world.observe(&mut edges, false, false), [Sfx::StructureCollapse]);
    }
}
