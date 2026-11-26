use super::{Recipe, Screen};
use crate::eval::metrics::Span;

fn recipe() -> Recipe {
    serde_json::from_str(
        r#"{
          "schema_version": 1, "id": "demo", "title": "Demo",
          "width": 640, "height": 360, "fps": 10, "duration_secs": 20.0,
          "stills": [
            {"t": 0.0, "terminal": {"lines": ["$ "]}},
            {"t": 4.0, "step": "01-init", "terminal": {"type": "git init"}},
            {"t": 6.0, "step": "01-init", "terminal": {"output": ["done"]}},
            {"t": 7.0, "step": "01-init", "continuous": true, "terminal": {"output": ["more"]}},
            {"t": 12.0, "step": "02-recap", "slide": {"title": "Recap", "bullets": ["a"]}},
            {"t": 16.0, "desktop": {}}
          ],
          "narration": [{"t": 0.5, "text": "Hello."}, {"t": 4.5, "text": "Run git init."}]
        }"#,
    )
    .unwrap()
}

#[test]
fn a_well_formed_recipe_validates_and_round_trips() {
    let recipe = recipe();
    recipe.validate().unwrap();
    assert!(matches!(recipe.stills[1].screen, Screen::Terminal(_)));
    let json = serde_json::to_string(&recipe).unwrap();
    assert_eq!(serde_json::from_str::<Recipe>(&json).unwrap(), recipe);
}

#[test]
fn change_times_skip_the_first_and_continuous_stills() {
    assert_eq!(recipe().change_times(), vec![4.0, 6.0, 12.0, 16.0]);
}

#[test]
fn step_spans_run_from_first_still_to_the_next_label() {
    let spans = recipe().step_spans();
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].id, "01-init");
    assert_eq!(spans[0].span, Span::new(4.0, 12.0));
    assert_eq!(spans[1].span, Span::new(12.0, 16.0));
}

#[test]
fn still_durations_cover_the_whole_video() {
    let durations = recipe().still_durations();
    assert_eq!(durations, vec![4.0, 2.0, 1.0, 5.0, 4.0, 4.0]);
    assert!((durations.iter().sum::<f64>() - 20.0).abs() < 1e-9);
}

#[test]
fn stills_out_of_order_or_past_the_end_are_rejected() {
    let mut late = recipe();
    late.stills[2].t = 3.0;
    assert!(late
        .validate()
        .unwrap_err()
        .to_string()
        .contains("increase"));
    let mut past = recipe();
    past.stills[5].t = 25.0;
    assert!(past
        .validate()
        .unwrap_err()
        .to_string()
        .contains("before the end"));
}

#[test]
fn a_step_label_used_in_two_runs_is_rejected() {
    let mut split = recipe();
    split.stills[5].step = Some("01-init".into());
    let err = split.validate().unwrap_err().to_string();
    assert!(err.contains("two separate runs"), "got: {err}");
}
