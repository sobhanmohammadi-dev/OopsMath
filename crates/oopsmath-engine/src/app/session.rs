//! In-game session lifecycle: menu, controls help, stage completion, replay
//! and exit.
//!
//! This module is pure state logic (no entities, no input). The HUD module
//! `ui::hud::session` reads input, calls these methods and draws the result.
//!
//! * One overlay at a time ([`SessionOverlay`]), so a menu, the controls help
//!   and the completion panel can never be shown together.
//! * Completion is latched per stage run: [`SessionUi::try_complete`] returns
//!   `true` once, until the session is reset by entering or leaving
//!   `AppState::InGame`.
//! * Replay and exit never reset stage state by hand. They only return the
//!   next [`AppState`]; the existing `OnExit(InGame)` / `OnEnter(InGame)`
//!   cleanup and the `LoadingStage` load path do the rest.
//! * After a transition has been requested, `leaving` is set and every further
//!   request is ignored, so two requests in the same frame cannot disagree.

use bevy::prelude::*;

use super::states::AppState;

/// Which full-screen panel is open, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SessionOverlay {
    #[default]
    None,
    /// Pause-style menu: Resume, Controls, Restart, Back to Stages.
    Menu,
    /// Controls help (opened from the menu).
    Help,
    /// Shown once when every stage objective is complete.
    Complete,
}

/// A button on one of the overlays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAction {
    Resume,
    ShowControls,
    /// Help -> Menu.
    BackToMenu,
    /// Completion panel -> keep playing the finished stage.
    KeepBuilding,
    /// Reload the current stage through the normal loading flow.
    Restart,
    BackToStages,
}

/// Stage-run UI state. Reset whenever `InGame` is entered or left.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct SessionUi {
    overlay: SessionOverlay,
    completion_shown: bool,
    leaving: bool,
}

impl SessionUi {
    pub fn overlay(&self) -> SessionOverlay {
        self.overlay
    }

    pub fn is_leaving(&self) -> bool {
        self.leaving
    }

    /// True while placement, rotation and the hammer must not run.
    pub fn blocks_world(&self) -> bool {
        self.leaving || self.overlay != SessionOverlay::None
    }

    /// Escape or the Menu button. Opens the menu only when no other popup is
    /// open; closes the menu or the completion panel; steps Help back to the
    /// menu. Ignored once a transition has been requested.
    pub fn toggle_menu(&mut self, other_popup_open: bool) {
        if self.leaving {
            return;
        }
        self.overlay = match self.overlay {
            SessionOverlay::Menu | SessionOverlay::Complete => SessionOverlay::None,
            SessionOverlay::Help => SessionOverlay::Menu,
            SessionOverlay::None if other_popup_open => SessionOverlay::None,
            SessionOverlay::None => SessionOverlay::Menu,
        };
    }

    /// Read-only check, so the per-frame system does not touch change
    /// detection while nothing happens. Shown at most once per run. It does
    /// not wait for the Shop / Math Challenge / Plan popups (the player solves
    /// the last challenge *inside* a popup); the HUD system closes them when
    /// this fires. It does wait for the menu overlay.
    pub fn wants_completion(&self, all_complete: bool) -> bool {
        all_complete
            && !self.completion_shown
            && !self.leaving
            && self.overlay == SessionOverlay::None
    }

    /// Latches and opens the completion panel. Returns `true` exactly once per
    /// run.
    pub fn try_complete(&mut self, all_complete: bool) -> bool {
        if !self.wants_completion(all_complete) {
            return false;
        }
        self.completion_shown = true;
        self.overlay = SessionOverlay::Complete;
        true
    }

    /// Applies a button press. Returns the state to switch to when the action
    /// leaves the current run. An action that does not belong to the open
    /// overlay (for example a stale press) is ignored.
    pub fn apply(&mut self, action: SessionAction) -> Option<AppState> {
        use SessionOverlay as O;
        if self.leaving {
            return None;
        }
        match (action, self.overlay) {
            (SessionAction::Resume, O::Menu) | (SessionAction::KeepBuilding, O::Complete) => {
                self.overlay = O::None;
                None
            }
            (SessionAction::ShowControls, O::Menu) => {
                self.overlay = O::Help;
                None
            }
            (SessionAction::BackToMenu, O::Help) => {
                self.overlay = O::Menu;
                None
            }
            (SessionAction::Restart, O::Menu | O::Complete) => {
                self.leaving = true;
                Some(AppState::LoadingStage)
            }
            (SessionAction::BackToStages, O::Menu | O::Complete) => {
                self.leaving = true;
                Some(AppState::StageBrowser)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_opens_and_closes_the_menu_only_without_another_popup() {
        let mut s = SessionUi::default();
        s.toggle_menu(true);
        assert_eq!(s.overlay(), SessionOverlay::None, "a popup owns Escape");
        assert!(!s.blocks_world());
        s.toggle_menu(false);
        assert_eq!(s.overlay(), SessionOverlay::Menu);
        assert!(s.blocks_world());
        s.toggle_menu(false);
        assert_eq!(s.overlay(), SessionOverlay::None);
    }

    #[test]
    fn escape_in_help_returns_to_the_menu() {
        let mut s = SessionUi::default();
        s.toggle_menu(false);
        assert_eq!(s.apply(SessionAction::ShowControls), None);
        assert_eq!(s.overlay(), SessionOverlay::Help);
        s.toggle_menu(false);
        assert_eq!(s.overlay(), SessionOverlay::Menu);
        assert_eq!(s.apply(SessionAction::Resume), None);
        assert_eq!(s.overlay(), SessionOverlay::None);
    }

    #[test]
    fn incomplete_objectives_never_trigger_completion() {
        let mut s = SessionUi::default();
        assert!(!s.try_complete(false));
        assert_eq!(s.overlay(), SessionOverlay::None);
        assert_eq!(s, SessionUi::default(), "nothing changed");
    }

    #[test]
    fn completion_fires_once_per_run() {
        let mut s = SessionUi::default();
        assert!(s.try_complete(true));
        assert_eq!(s.overlay(), SessionOverlay::Complete);
        for _ in 0..10 {
            assert!(!s.try_complete(true), "latched");
        }
        // Closing the panel does not bring it back.
        assert_eq!(s.apply(SessionAction::KeepBuilding), None);
        assert_eq!(s.overlay(), SessionOverlay::None);
        assert!(!s.try_complete(true));
        assert!(!s.blocks_world());
    }

    #[test]
    fn completion_waits_for_the_menu_then_still_fires_once() {
        let mut s = SessionUi::default();
        s.toggle_menu(false);
        assert!(!s.try_complete(true), "the menu is open");
        assert_eq!(s.overlay(), SessionOverlay::Menu);
        s.toggle_menu(false);
        assert!(s.try_complete(true));
        assert!(!s.try_complete(true));
    }

    #[test]
    fn a_new_run_can_complete_again() {
        let mut s = SessionUi::default();
        assert!(s.try_complete(true));
        s = SessionUi::default(); // what entering or leaving InGame does
        assert!(s.try_complete(true));
    }

    #[test]
    fn restart_and_exit_request_the_existing_states() {
        let mut s = SessionUi::default();
        s.toggle_menu(false);
        assert_eq!(
            s.apply(SessionAction::Restart),
            Some(AppState::LoadingStage)
        );
        let mut s = SessionUi::default();
        s.toggle_menu(false);
        assert_eq!(
            s.apply(SessionAction::BackToStages),
            Some(AppState::StageBrowser)
        );
        let mut s = SessionUi::default();
        assert!(s.try_complete(true));
        assert_eq!(
            s.apply(SessionAction::Restart),
            Some(AppState::LoadingStage)
        );
        let mut s = SessionUi::default();
        assert!(s.try_complete(true));
        assert_eq!(
            s.apply(SessionAction::BackToStages),
            Some(AppState::StageBrowser)
        );
    }

    #[test]
    fn only_the_first_transition_in_a_frame_wins() {
        let mut s = SessionUi::default();
        s.toggle_menu(false);
        assert_eq!(
            s.apply(SessionAction::Restart),
            Some(AppState::LoadingStage)
        );
        assert!(s.is_leaving());
        assert!(
            s.blocks_world(),
            "the world stays blocked until the state changes"
        );
        assert_eq!(s.apply(SessionAction::BackToStages), None);
        assert_eq!(s.apply(SessionAction::Restart), None);
        s.toggle_menu(false);
        assert_eq!(
            s.overlay(),
            SessionOverlay::Menu,
            "Escape is ignored while leaving"
        );
        assert!(!s.try_complete(true), "no completion while leaving");
    }

    #[test]
    fn actions_that_do_not_belong_to_the_open_overlay_are_ignored() {
        let mut s = SessionUi::default();
        // No overlay: nothing can transition.
        assert_eq!(s.apply(SessionAction::Restart), None);
        assert_eq!(s.apply(SessionAction::BackToStages), None);
        assert!(!s.is_leaving());
        // Help only offers "back".
        s.toggle_menu(false);
        s.apply(SessionAction::ShowControls);
        assert_eq!(s.apply(SessionAction::Restart), None);
        assert_eq!(s.apply(SessionAction::BackToStages), None);
        assert_eq!(s.overlay(), SessionOverlay::Help);
        // The completion panel has no Resume / Controls.
        let mut c = SessionUi::default();
        assert!(c.try_complete(true));
        assert_eq!(c.apply(SessionAction::Resume), None);
        assert_eq!(c.apply(SessionAction::ShowControls), None);
        assert_eq!(c.overlay(), SessionOverlay::Complete);
    }
}
