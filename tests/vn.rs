//! Story-engine integration tests against the real `assets/scripts`.

use std::collections::BTreeMap;

use cuda_hearts::vn::script::Script;
use cuda_hearts::vn::state::{Flow, Playhead, VnError};

const PROLOGUE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/scripts/prologue.vn");

fn load_prologue() -> BTreeMap<String, Script> {
    let source = std::fs::read_to_string(PROLOGUE_PATH)
        .unwrap_or_else(|e| panic!("cannot read {PROLOGUE_PATH}: {e}"));
    let script = Script::parse(&source).unwrap_or_else(|e| panic!("{PROLOGUE_PATH}: {e}"));
    BTreeMap::from([("prologue".to_string(), script)])
}

/// Drive a run to its ending, auto-picking the first option at every choice.
fn play_to_ending(playhead: &mut Playhead, scripts: &BTreeMap<String, Script>) -> String {
    for _ in 0..1000 {
        match playhead.next(scripts) {
            Ok(Flow::Line { .. }) => {}
            Ok(Flow::Choice(options)) => {
                assert!(!options.is_empty(), "empty choice block");
                playhead
                    .pick(scripts, 0)
                    .unwrap_or_else(|e| panic!("pick failed: {e}"));
            }
            Ok(Flow::Ending(name)) => return name,
            Ok(Flow::Done) => panic!("run finished without an ending"),
            Err(e @ VnError::UnknownLabel(_)) | Err(e @ VnError::Runaway) => {
                panic!("story error: {e}")
            }
            Err(e) => panic!("unexpected story error: {e}"),
        }
    }
    panic!("runaway story loop in prologue");
}

#[test]
fn prologue_plays_to_its_ending() {
    let scripts = load_prologue();
    let mut playhead = Playhead::start("prologue");
    let ending = play_to_ending(&mut playhead, &scripts);
    assert_eq!(ending, "first_keynote");
    assert_eq!(playhead.ending.as_deref(), Some("first_keynote"));
    assert!(playhead.affinity != 0, "prologue should move affinity");
    assert!(
        playhead
            .flags
            .iter()
            .any(|f| f == "cuda_rizz" || f == "jacket_lore"),
        "flirt/jacket branch should set a flag"
    );
}

#[test]
fn prologue_choice_paths_all_reach_an_ending() {
    let scripts = load_prologue();
    for pick in [0usize, 1, 2] {
        let mut playhead = Playhead::start("prologue");
        let mut picked = false;
        let mut ending = String::new();
        for _ in 0..1000 {
            match playhead.next(&scripts) {
                Ok(Flow::Line { .. }) => {}
                Ok(Flow::Choice(_)) => {
                    let index = if picked { 0 } else { pick };
                    playhead
                        .pick(&scripts, index)
                        .unwrap_or_else(|e| panic!("pick {index} failed: {e}"));
                    picked = true;
                }
                Ok(Flow::Ending(name)) => {
                    ending = name;
                    break;
                }
                Ok(Flow::Done) => panic!("choice path {pick} finished without ending"),
                Err(e) => panic!("choice path {pick} failed: {e}"),
            }
        }
        assert_eq!(
            ending, "first_keynote",
            "choice path {pick} must reach the ending"
        );
    }
}

#[test]
fn save_round_trips_mid_story() {
    let scripts = load_prologue();
    let mut playhead = Playhead::start("prologue");

    // Walk a few steps to get mid-story state.
    for _ in 0..3 {
        let _ = playhead.next(&scripts);
    }
    let snapshot = playhead.to_save();
    let restored = Playhead::from_save(snapshot);

    // The restored run must play identically to the original from here on.
    let mut original = playhead.clone();
    let a = play_to_ending(&mut original, &scripts);
    let b = play_to_ending(&mut { restored }, &scripts);
    assert_eq!(a, b);
}

#[test]
fn every_script_in_assets_parses() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/scripts");
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {dir}: {e}"));
    let mut checked = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "vn") {
            continue;
        }
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        Script::parse(&source).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        checked += 1;
    }
    assert!(checked > 0, "no .vn scripts found in {dir}");
}
