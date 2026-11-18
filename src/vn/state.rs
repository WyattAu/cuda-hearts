//! The playhead: runtime state machine driving one play-through.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::vn::script::{ChoiceOption, Script, ScriptError, Step};

/// Transcript lines kept in memory.
const TRANSCRIPT_CAP: usize = 200;

/// Jump-hop budget per [`Playhead::next`] call; guards against label
/// ping-pong (e.g. `~ jump a` / `~ label a` loops through pass-through
/// steps forever).
const MAX_HOPS: u32 = 10_000;

/// What the presentation layer should show after [`Playhead::next`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Flow {
    /// A dialogue line; `speaker` is a cast id or [`crate::vn::cast::NARRATOR`].
    Line { speaker: String, text: String },
    /// A choice the player must answer before the story continues.
    Choice(Vec<ChoiceOption>),
    /// An ending was reached; carries the ending id.
    Ending(String),
    /// No more steps.
    Done,
}

/// Errors raised while driving a script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VnError {
    /// A jump or choice target named a label that does not exist.
    UnknownLabel(String),
    /// [`Playhead::next`] was called while a choice is still pending.
    ChoicePending,
    /// [`Playhead::pick`] was called with an out-of-range or stale index.
    InvalidChoice(usize),
    /// Too many jumps in a single step — likely an infinite label loop.
    Runaway,
    /// The script failed to parse.
    Parse(ScriptError),
    /// The playhead names a script that is not loaded.
    UnknownScript(String),
}

impl fmt::Display for VnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VnError::UnknownLabel(label) => write!(f, "unknown label `{label}`"),
            VnError::ChoicePending => write!(f, "a choice is pending — call pick() first"),
            VnError::InvalidChoice(i) => write!(f, "invalid choice index {i}"),
            VnError::Runaway => write!(f, "too many jumps — infinite label loop?"),
            VnError::Parse(e) => write!(f, "{e}"),
            VnError::UnknownScript(id) => write!(f, "script `{id}` is not loaded"),
        }
    }
}

impl std::error::Error for VnError {}

impl From<ScriptError> for VnError {
    fn from(e: ScriptError) -> Self {
        VnError::Parse(e)
    }
}

/// A character standing on stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageActor {
    pub cast: String,
    pub mood: String,
}

/// One run of one script: playhead, stats, stage, transcript.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Playhead {
    /// Id of the script this run plays (key into the loaded-script map).
    pub script_id: String,
    /// Index of the next step to execute.
    pub pc: usize,
    /// Relationship score; moved by `~ affinity <+n|-n>`.
    pub affinity: i64,
    /// Story flags set by `~ flag <name>`.
    pub flags: BTreeSet<String>,
    /// Background set by the last `~ scene` directive.
    pub scene: Option<String>,
    /// Characters currently on stage, in show order.
    pub stage: Vec<StageActor>,
    /// Recent lines, oldest first, capped at [`TRANSCRIPT_CAP`].
    pub transcript: VecDeque<String>,
    /// Set once an ending was reached.
    pub ending: Option<String>,
    pending: Option<Vec<ChoiceOption>>,
}

impl Playhead {
    /// A fresh run of `script_id`, starting at step 0.
    pub fn start(script_id: impl Into<String>) -> Self {
        Self {
            script_id: script_id.into(),
            ..Self::default()
        }
    }

    /// Advance to the next presentable step, executing pass-through
    /// directives (`scene`, `show`, `hide`, `label`, `jump`, `affinity`,
    /// `flag`) along the way.
    ///
    /// Returns [`Flow::Choice`] once and then refuses (with
    /// [`VnError::ChoicePending`]) until [`Playhead::pick`] resolves it.
    pub fn next(&mut self, scripts: &BTreeMap<String, Script>) -> Result<Flow, VnError> {
        if self.pending.is_some() {
            return Err(VnError::ChoicePending);
        }
        if self.ending.is_some() {
            return Ok(Flow::Done);
        }
        let script = scripts
            .get(&self.script_id)
            .ok_or_else(|| VnError::UnknownScript(self.script_id.clone()))?;

        let mut hops = 0u32;
        loop {
            let Some(step) = script.steps.get(self.pc).cloned() else {
                return Ok(Flow::Done);
            };
            match step {
                Step::Say { speaker, text } => {
                    self.push_transcript(format!("{speaker}: {text}"));
                    self.pc += 1;
                    return Ok(Flow::Line { speaker, text });
                }
                Step::Choice(options) => {
                    self.pc += 1;
                    self.pending = Some(options.clone());
                    return Ok(Flow::Choice(options));
                }
                Step::Ending(name) => {
                    self.pc += 1;
                    self.ending = Some(name.clone());
                    return Ok(Flow::Ending(name));
                }
                Step::Scene(id) => self.scene = Some(id),
                Step::Show { cast, mood } => {
                    self.stage.retain(|a| a.cast != cast);
                    self.stage.push(StageActor { cast, mood });
                }
                Step::Hide(cast) => self.stage.retain(|a| a.cast != cast),
                Step::Label(_) => {}
                Step::Jump(target) => {
                    let idx = *script
                        .labels
                        .get(&target)
                        .ok_or(VnError::UnknownLabel(target))?;
                    self.pc = idx;
                    hops += 1;
                    if hops > MAX_HOPS {
                        return Err(VnError::Runaway);
                    }
                    continue; // land on the label step; skip the pc += 1 below
                }
                Step::Affinity(delta) => self.affinity += i64::from(delta),
                Step::Flag(name) => {
                    self.flags.insert(name);
                }
            }
            self.pc += 1;
        }
    }

    /// Answer the pending choice. `option` indexes into the options last
    /// returned as [`Flow::Choice`].
    pub fn pick(
        &mut self,
        scripts: &BTreeMap<String, Script>,
        option: usize,
    ) -> Result<(), VnError> {
        let Some(options) = self.pending.clone() else {
            return Err(VnError::InvalidChoice(option));
        };
        let Some(chosen) = options.get(option) else {
            return Err(VnError::InvalidChoice(option));
        };
        let script = scripts
            .get(&self.script_id)
            .ok_or_else(|| VnError::UnknownScript(self.script_id.clone()))?;
        let idx = *script
            .labels
            .get(&chosen.target)
            .ok_or_else(|| VnError::UnknownLabel(chosen.target.clone()))?;
        self.pending = None;
        self.pc = idx;
        Ok(())
    }

    /// The pending choice options, if any.
    pub fn pending_options(&self) -> Option<&[ChoiceOption]> {
        self.pending.as_deref()
    }

    fn push_transcript(&mut self, line: String) {
        if self.transcript.len() >= TRANSCRIPT_CAP {
            self.transcript.pop_front();
        }
        self.transcript.push_back(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vn::script::Script;

    fn loaded(source: &str) -> BTreeMap<String, Script> {
        let script = Script::parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        BTreeMap::from([("test".to_string(), script)])
    }

    #[test]
    fn walks_lines_then_reports_done() {
        let scripts = loaded("jensen: One.\njensen: Two.\n");
        let mut p = Playhead::start("test");
        assert_eq!(
            p.next(&scripts),
            Ok(Flow::Line {
                speaker: "jensen".into(),
                text: "One.".into()
            })
        );
        assert_eq!(
            p.next(&scripts),
            Ok(Flow::Line {
                speaker: "jensen".into(),
                text: "Two.".into()
            })
        );
        assert_eq!(p.next(&scripts), Ok(Flow::Done));
    }

    #[test]
    fn applies_passthrough_directives() {
        let scripts =
            loaded("~ scene lab\n~ show jensen happy\n~ affinity +2\n~ flag x\njensen: Hi.\n");
        let mut p = Playhead::start("test");
        let _ = p.next(&scripts);
        assert_eq!(p.scene.as_deref(), Some("lab"));
        assert_eq!(
            p.stage,
            vec![StageActor {
                cast: "jensen".into(),
                mood: "happy".into()
            }]
        );
        assert_eq!(p.affinity, 2);
        assert!(p.flags.contains("x"));
    }

    #[test]
    fn choice_blocks_until_pick_then_branches() {
        let scripts = loaded(
            "narrator: Go.\n* Left -> left\n* Right -> right\n~ label left\n~ affinity +1\njensen: Left it is.\n~ label right\n~ ending right_end\n",
        );
        let mut p = Playhead::start("test");
        assert!(
            matches!(p.next(&scripts), Ok(Flow::Line { .. })),
            "first step is the narrator line"
        );
        assert!(
            matches!(p.next(&scripts), Ok(Flow::Choice(_))),
            "second step is the choice"
        );
        assert_eq!(p.next(&scripts), Err(VnError::ChoicePending));
        p.pick(&scripts, 1)
            .unwrap_or_else(|e| panic!("pick failed: {e}"));
        assert_eq!(p.next(&scripts), Ok(Flow::Ending("right_end".into())));
        assert_eq!(p.next(&scripts), Ok(Flow::Done));
    }

    #[test]
    fn pick_out_of_range_is_invalid_but_keeps_pending() {
        let scripts = loaded("* Only -> a\n~ label a\njensen: Hi.\n");
        let mut p = Playhead::start("test");
        let _ = p.next(&scripts);
        assert_eq!(p.pick(&scripts, 5), Err(VnError::InvalidChoice(5)));
        assert!(p.pending_options().is_some());
    }

    #[test]
    fn jump_to_missing_label_errors() {
        let scripts = loaded("~ jump nowhere\n");
        let mut p = Playhead::start("test");
        assert_eq!(
            p.next(&scripts),
            Err(VnError::UnknownLabel("nowhere".into()))
        );
    }

    #[test]
    fn self_jump_is_runaway_not_hang() {
        let scripts = loaded("~ label a\n~ jump a\n");
        let mut p = Playhead::start("test");
        assert_eq!(p.next(&scripts), Err(VnError::Runaway));
    }

    #[test]
    fn unknown_script_is_reported() {
        let mut p = Playhead::start("missing");
        assert_eq!(
            p.next(&BTreeMap::new()),
            Err(VnError::UnknownScript("missing".into()))
        );
    }
}
