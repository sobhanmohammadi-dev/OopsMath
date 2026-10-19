use serde_json::json;

use super::answer::{parse_decimal, parse_integer};
use super::*;
use crate::blocks::PlacedWorld;
use crate::items::ItemRef;
use crate::items::economy::Wallet;
use crate::items::inventory::Inventory;

fn exact(value: serde_json::Value) -> AnswerSpec {
    AnswerSpec::from_stage(&json!({"mode": "exact", "value": value}))
        .unwrap()
        .unwrap()
}

// ------------------------------------------------------------ integers

#[test]
fn integer_answers_compare_numerically() {
    let spec = exact(json!(12));
    assert_eq!(spec, AnswerSpec::Integer(12));
    for ok in ["12", " 12 ", "+12", "\t12\n", "0012"] {
        assert_eq!(spec.check(ok), Ok(true), "{ok:?}");
    }
    assert_eq!(spec.check("13"), Ok(false));
    assert_eq!(exact(json!(-3)).check("-3"), Ok(true));
    assert_eq!(exact(json!(0)).check("-0"), Ok(true));
}

#[test]
fn bad_integer_input_is_an_error_not_a_wrong_answer() {
    let spec = exact(json!(12));
    assert_eq!(spec.check(""), Err(AnswerError::Empty));
    assert_eq!(spec.check("   "), Err(AnswerError::Empty));
    for bad in [
        "abc", "12.0", "1e3", "1 2", "--1", "+", "-", "0x10", "12a", "1,5", "NaN",
    ] {
        assert_eq!(spec.check(bad), Err(AnswerError::Malformed), "{bad:?}");
    }
    assert_eq!(
        spec.check("99999999999999999999"),
        Err(AnswerError::OutOfRange)
    );
    assert_eq!(parse_integer("9223372036854775807"), Ok(i64::MAX));
    assert_eq!(
        parse_integer("9223372036854775808"),
        Err(AnswerError::OutOfRange)
    );
}

// ------------------------------------------------------------ decimals

#[test]
fn decimal_answers_use_exact_f64_equality_without_epsilon() {
    let spec = exact(json!(2.5));
    assert_eq!(
        spec,
        AnswerSpec::Decimal {
            value: 2.5,
            tolerance: 0.0
        }
    );
    for ok in ["2.5", "2.50", "+2.5", " 2.5 ", "02.5"] {
        assert_eq!(spec.check(ok), Ok(true), "{ok:?}");
    }
    assert_eq!(spec.check("2.4"), Ok(false));
    assert_eq!(spec.check("2.5000001"), Ok(false), "no hidden tolerance");
    // A decimal expectation also accepts the integer spelling of the same value.
    assert_eq!(exact(json!(3.0)).check("3"), Ok(true));
}

#[test]
fn malformed_decimal_input_is_rejected() {
    let spec = exact(json!(2.5));
    assert_eq!(spec.check(""), Err(AnswerError::Empty));
    for bad in [
        ".5", "5.", "1e3", "1.2.3", "inf", "-inf", "NaN", "2,5", "2.5x", "+-2", ".",
    ] {
        assert_eq!(spec.check(bad), Err(AnswerError::Malformed), "{bad:?}");
    }
    let huge = "9".repeat(400);
    assert_eq!(
        parse_decimal(&huge),
        Err(AnswerError::OutOfRange),
        "overflow to infinity"
    );
}

#[test]
fn tolerance_is_explicit_and_validated() {
    let spec =
        AnswerSpec::from_stage(&json!({"mode": "tolerance", "value": 3.14, "tolerance": 0.01}))
            .unwrap()
            .unwrap();
    assert_eq!(spec.check("3.145"), Ok(true));
    assert_eq!(spec.check("3.14"), Ok(true));
    assert_eq!(spec.check("3.2"), Ok(false));
    for bad in [
        json!({"mode": "tolerance", "value": 3.14, "tolerance": -0.1}),
        json!({"mode": "tolerance", "value": 3.14}),
        json!({"mode": "tolerance", "tolerance": 0.1}),
        json!({"mode": "tolerance", "value": "x", "tolerance": 0.1}),
        json!({"mode": "exact"}),
        json!({"mode": "exact", "value": "12"}),
        json!({"mode": "exact", "value": 18446744073709551615u64}),
        json!({"value": 1}),
    ] {
        assert!(AnswerSpec::from_stage(&bad).is_err(), "{bad}");
    }
    // Valid modes that v1 does not support are not errors.
    assert_eq!(
        AnswerSpec::from_stage(&json!({"mode": "expression"})),
        Ok(None)
    );
    assert_eq!(
        AnswerSpec::from_stage(&json!({"mode": "multiple_choice", "value": 1})),
        Ok(None)
    );
}

// ------------------------------------------------------- stage parsing

fn learning() -> serde_json::Value {
    json!({"question": "stage.a.question"})
}

fn math_task(id: &str, answer: serde_json::Value) -> serde_json::Value {
    json!({"id": id, "type": "math", "math": {"type": "multiplication", "answer": answer}})
}

fn parse(tasks: serde_json::Value) -> Result<(Vec<Challenge>, Vec<String>), ChallengeError> {
    let translate = |k: &str| (k == "stage.a.question").then(|| "How many bricks?".to_string());
    StageChallenges::parse(&json!({ "tasks": tasks }), &learning(), &translate)
}

#[test]
fn stage_001_style_math_task_becomes_a_challenge() {
    let (list, skipped) = parse(json!([
        math_task("t_math", json!({"mode": "exact", "value": 12})),
        {"id": "t_build", "type": "construction", "target": "wall_a", "depends_on": ["t_math"]},
    ]))
    .unwrap();
    assert!(skipped.is_empty());
    assert_eq!(list.len(), 1);
    assert_eq!(
        (list[0].id.as_str(), list[0].prompt.as_str()),
        ("t_math", "How many bricks?")
    );
    assert_eq!(list[0].answer, AnswerSpec::Integer(12));
}

#[test]
fn prompts_come_from_math_prompt_then_learning_question() {
    let mut task = math_task("a", json!({"mode": "exact", "value": 1}));
    task["math"]["prompt"] = json!("own.key");
    let (list, _) = parse(json!([task])).unwrap();
    assert_eq!(
        list[0].prompt, "own.key",
        "untranslated keys are shown as-is"
    );
    let no_prompt = StageChallenges::parse(
        &json!({"tasks": [math_task("a", json!({"mode": "exact", "value": 1}))]}),
        &json!({}),
        &|_| None,
    );
    assert_eq!(no_prompt, Err(ChallengeError::NoPrompt("a".into())));
}

#[test]
fn duplicate_ids_and_bad_answers_are_rejected() {
    let a = math_task("a", json!({"mode": "exact", "value": 1}));
    assert_eq!(
        parse(json!([a.clone(), a])),
        Err(ChallengeError::DuplicateId("a".into()))
    );
    assert!(matches!(
        parse(json!([math_task(
            "a",
            json!({"mode": "tolerance", "value": 1, "tolerance": -1})
        )])),
        Err(ChallengeError::InvalidAnswer { .. })
    ));
}

#[test]
fn stages_without_challenges_and_unsupported_modes_are_handled() {
    assert_eq!(parse(json!([])).unwrap().0.len(), 0);
    assert_eq!(
        StageChallenges::parse(&json!({}), &json!({}), &|_| None)
            .unwrap()
            .0
            .len(),
        0
    );
    let (list, skipped) = parse(json!([
        math_task("g", json!({"mode": "expression", "expression": "x+1"})),
        {"id": "no_answer", "type": "math", "math": {"type": "multiplication", "generator": {}}},
        math_task("ok", json!({"mode": "exact", "value": 2})),
    ]))
    .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(skipped, ["g"]);
}

// ----------------------------------------------------- runtime solving

fn stage(values: &[(&str, i64)]) -> (StageChallenges, ChallengeTracker) {
    let tasks: Vec<_> = values
        .iter()
        .map(|(id, v)| math_task(id, json!({"mode": "exact", "value": v})))
        .collect();
    let (challenges, _) = parse(json!(tasks)).unwrap();
    let mut tracker = ChallengeTracker::default();
    tracker.reset(challenges.len());
    (
        StageChallenges {
            challenges,
            error: None,
        },
        tracker,
    )
}

#[test]
fn invalid_and_wrong_answers_never_solve() {
    let (c, mut t) = stage(&[("a", 12)]);
    assert_eq!(submit(&c, &mut t, "a", "13"), Ok(SubmitOutcome::Incorrect));
    assert_eq!(
        submit(&c, &mut t, "a", ""),
        Err(SubmitError::Answer(AnswerError::Empty))
    );
    assert_eq!(
        submit(&c, &mut t, "a", "twelve"),
        Err(SubmitError::Answer(AnswerError::Malformed))
    );
    assert_eq!(
        submit(&c, &mut t, "a", "99999999999999999999"),
        Err(SubmitError::Answer(AnswerError::OutOfRange))
    );
    assert!(!t.is_solved(&c, "a"));
}

#[test]
fn a_correct_answer_solves_exactly_once() {
    let (c, mut t) = stage(&[("a", 12), ("b", 5)]);
    assert_eq!(submit(&c, &mut t, "a", " 12 "), Ok(SubmitOutcome::Solved));
    assert!(t.is_solved(&c, "a") && !t.is_solved(&c, "b"));
    // Duplicate submissions (even a wrong or malformed one) cannot transition again.
    assert_eq!(
        submit(&c, &mut t, "a", "12"),
        Ok(SubmitOutcome::AlreadySolved)
    );
    assert_eq!(
        submit(&c, &mut t, "a", "nonsense"),
        Ok(SubmitOutcome::AlreadySolved)
    );
    assert!(t.is_solved(&c, "a"));
    assert_eq!(
        submit(&c, &mut t, "zzz", "1"),
        Err(SubmitError::UnknownChallenge)
    );
}

#[test]
fn solved_state_does_not_leak_into_another_stage() {
    let (c, mut t) = stage(&[("t_math", 12)]);
    submit(&c, &mut t, "t_math", "12").unwrap();
    assert!(t.is_solved(&c, "t_math"));
    // New stage with the same challenge id: tracker is rebuilt from scratch.
    let (c2, _) = stage(&[("t_math", 7)]);
    t.reset(c2.challenges.len());
    assert!(!t.is_solved(&c2, "t_math"));
    // And unloading clears everything.
    t.reset(0);
    assert!(!t.is_solved(&c, "t_math"));
}

#[test]
fn solving_touches_no_economy_inventory_or_world_state() {
    let (c, mut t) = stage(&[("a", 12)]);
    let wallet = Wallet::new(40);
    let mut inventory = Inventory::default();
    inventory
        .add(ItemRef::Block(crate::blocks::models::wood_log_id()), 3)
        .unwrap();
    let world = PlacedWorld::default();
    let (w0, i0) = (wallet, inventory.clone());
    submit(&c, &mut t, "a", "12").unwrap();
    assert_eq!((wallet, inventory), (w0, i0));
    assert!(world.is_empty());
}

// ------------------------------------------------------------- panel

#[test]
fn the_panel_filters_input_and_refuses_empty_submissions() {
    let (c, mut t) = stage(&[("a", 12)]);
    let mut panel = ChallengePanel::default();
    panel.open_for(&c, &t);
    assert!(panel.open && panel.index == 0);
    for ch in "1x2 q".chars() {
        panel.push_char(ch);
    }
    assert_eq!(panel.input, "12");
    panel.backspace();
    panel.backspace();
    panel.backspace();
    assert_eq!(panel.input, "");
    panel.submit(&c, &mut t);
    assert_eq!(panel.feedback.as_deref(), Some("Type an answer first."));
    assert!(!t.is_solved(&c, "a"));
    for _ in 0..40 {
        panel.push_char('1');
    }
    assert_eq!(panel.input.len(), MAX_INPUT_LEN);
}

#[test]
fn the_panel_solves_through_the_validated_path() {
    let (c, mut t) = stage(&[("a", 12), ("b", 3)]);
    let mut panel = ChallengePanel::default();
    panel.open_for(&c, &t);
    "13".chars().for_each(|ch| panel.push_char(ch));
    panel.submit(&c, &mut t);
    assert_eq!(panel.feedback.as_deref(), Some("Not correct. Try again."));
    assert!(!t.is_solved(&c, "a") && panel.input == "13");
    panel.input = "12".into();
    panel.submit(&c, &mut t);
    assert!(t.is_solved(&c, "a") && panel.input.is_empty());
    panel.submit(&c, &mut t);
    // Now the first unsolved challenge is "b".
    let mut again = ChallengePanel::default();
    again.open_for(&c, &t);
    assert_eq!(again.index, 1);
    again.step(1, 2);
    assert_eq!(again.index, 0);
    panel.close();
    assert_eq!(panel, ChallengePanel::default());
}

#[test]
fn a_stale_panel_cannot_submit_for_a_different_stage() {
    let (c, mut t) = stage(&[("a", 12)]);
    let mut panel = ChallengePanel {
        open: true,
        index: 3,
        input: "12".into(),
        feedback: None,
    };
    panel.submit(&c, &mut t);
    assert_eq!(
        panel.feedback.as_deref(),
        Some("This challenge is no longer available.")
    );
    assert!(!t.is_solved(&c, "a"));
}
