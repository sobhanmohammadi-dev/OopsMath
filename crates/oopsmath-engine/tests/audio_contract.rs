//! Contracts between the audio edge detector and the game code it observes.

use oopsmath_engine::audio::edges::INCORRECT_FEEDBACK;
use oopsmath_engine::audio::{Sfx, SfxVolumes};
use oopsmath_engine::challenges::{
    AnswerSpec, Challenge, ChallengePanel, ChallengeTracker, StageChallenges,
};

/// The wrong-answer sound is recognised by the feedback text the real
/// `ChallengePanel::submit` sets. If that text changes, this test fails instead
/// of the sound silently disappearing.
#[test]
fn the_wrong_answer_feedback_matches_what_the_game_sets() {
    let challenges = StageChallenges {
        challenges: vec![Challenge {
            id: "t_math".into(),
            prompt: "How many?".into(),
            answer: AnswerSpec::Integer(12),
        }],
        error: None,
    };
    let mut tracker = ChallengeTracker::default();
    tracker.reset(1);
    let mut panel = ChallengePanel::default();
    panel.open_for(&challenges, &tracker);

    panel.input = "13".into();
    panel.submit(&challenges, &mut tracker);
    assert_eq!(panel.feedback.as_deref(), Some(INCORRECT_FEEDBACK));
    assert!(!tracker.is_solved_at(0), "a wrong answer never solves");

    // Other results must not look like a wrong answer.
    panel.input.clear();
    panel.submit(&challenges, &mut tracker);
    assert_ne!(panel.feedback.as_deref(), Some(INCORRECT_FEEDBACK), "empty input");
    panel.input = "12".into();
    panel.submit(&challenges, &mut tracker);
    assert_ne!(panel.feedback.as_deref(), Some(INCORRECT_FEEDBACK), "correct answer");
    assert!(tracker.is_solved_at(0));
}

#[test]
fn the_catalog_files_and_levels_are_consistent() {
    let volumes = SfxVolumes::default();
    for sfx in Sfx::ALL {
        assert!(sfx.path().starts_with("audio/sfx/") && sfx.path().ends_with(".ogg"));
        assert!(volumes.gain(sfx) > 0.0 && volumes.gain(sfx) <= 1.0);
    }
}

/// Reports (does not fail) which expected files are not in `assets/audio/sfx`
/// yet, so the list of files still to supply is always one `cargo test` away:
/// `cargo test -p oopsmath-engine --test audio_contract -- --nocapture`.
#[test]
fn report_which_sound_files_are_present() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut missing = Vec::new();
    for sfx in Sfx::ALL {
        if !dir.join(sfx.path()).is_file() {
            missing.push(format!("assets/{}  ({})", sfx.path(), sfx.purpose()));
        }
    }
    if missing.is_empty() {
        println!("all {} sound effect files are present", Sfx::ALL.len());
    } else {
        println!("sound effect files still to supply:\n  {}", missing.join("\n  "));
    }
}
