//! VN script model and parser.
//!
//! A script is a line-based `.vn` file (see `assets/scripts/prologue.vn`):
//!
//! ```text
//! # comment
//! ~ scene gtc_backstage
//! ~ show jensen happy
//! ~ hide jensen
//! ~ label flirt
//! ~ jump flirt
//! ~ affinity +2
//! ~ flag cuda_rizz
//! ~ ending first_keynote
//! jensen: The more you love, the more you save.
//! * Flirt with silicon metaphors -> flirt
//! ```
//!
//! Consecutive `*` lines form a single choice block. Blank lines and `#`
//! comments are ignored. Speakers must be registered in [`crate::vn::cast`]
//! or be `narrator`.

use std::collections::BTreeMap;
use std::fmt;

use crate::vn::cast;

/// One parsed script instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// `~ scene <id>` — switch the background.
    Scene(String),
    /// `~ show <cast> [mood]` — put a character on stage.
    Show { cast: String, mood: String },
    /// `~ hide <cast>` — remove a character from the stage.
    Hide(String),
    /// `<speaker>: <text>` — a dialogue line.
    Say { speaker: String, text: String },
    /// A block of `* <text> -> <label>` options.
    Choice(Vec<ChoiceOption>),
    /// `~ label <name>` — a branch target.
    Label(String),
    /// `~ jump <name>` — go to a label.
    Jump(String),
    /// `~ affinity <+n|-n>` — adjust the relationship score.
    Affinity(i32),
    /// `~ flag <name>` — set a story flag.
    Flag(String),
    /// `~ ending <name>` — reach an ending.
    Ending(String),
}

/// One selectable option inside a [`Step::Choice`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceOption {
    /// Text shown on the button.
    pub text: String,
    /// Label to jump to when picked.
    pub target: String,
}

/// A parse error, with a 1-based line number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "script line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ScriptError {}

fn err(line: usize, message: impl Into<String>) -> ScriptError {
    ScriptError {
        line,
        message: message.into(),
    }
}

/// A parsed script: steps plus a `label -> step index` map.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Script {
    pub steps: Vec<Step>,
    pub labels: BTreeMap<String, usize>,
}

impl Script {
    /// Parse a `.vn` script.
    pub fn parse(input: &str) -> Result<Self, ScriptError> {
        let mut script = Script::default();
        let mut choices: Vec<ChoiceOption> = Vec::new();

        for (idx, raw) in input.lines().enumerate() {
            let line_no = idx + 1;
            let line = raw.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some(rest) = line.strip_prefix('*') {
                choices.push(parse_choice_line(rest, line_no)?);
                continue;
            }

            // Any non-choice line closes an open choice block.
            flush_choices(&mut choices, &mut script.steps);

            if let Some(directive) = line.strip_prefix('~') {
                parse_directive(directive, line_no, &mut script)?;
            } else if let Some((speaker, text)) = line.split_once(':') {
                let speaker = speaker.trim();
                let text = text.trim();
                if !cast::is_known_speaker(speaker) {
                    return Err(err(
                        line_no,
                        format!(
                            "unknown speaker `{speaker}` — add them to vn::cast::CAST (known: {})",
                            cast::CAST
                                .iter()
                                .map(|c| c.id)
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    ));
                }
                if text.is_empty() {
                    return Err(err(line_no, "dialogue text must not be empty"));
                }
                script.steps.push(Step::Say {
                    speaker: speaker.to_string(),
                    text: text.to_string(),
                });
            } else {
                return Err(err(
                    line_no,
                    "expected `~ directive`, `<speaker>: text` or `* choice -> label`",
                ));
            }
        }
        flush_choices(&mut choices, &mut script.steps);
        Ok(script)
    }

    /// The step a label points at.
    pub fn label_index(&self, label: &str) -> Option<usize> {
        self.labels.get(label).copied()
    }
}

fn parse_choice_line(rest: &str, line_no: usize) -> Result<ChoiceOption, ScriptError> {
    let Some((text, target)) = rest.split_once("->") else {
        return Err(err(line_no, "choice must be `* text -> label`"));
    };
    let text = text.trim();
    let target = target.trim();
    if text.is_empty() || target.is_empty() {
        return Err(err(line_no, "choice text and label must not be empty"));
    }
    Ok(ChoiceOption {
        text: text.to_string(),
        target: target.to_string(),
    })
}

fn flush_choices(choices: &mut Vec<ChoiceOption>, steps: &mut Vec<Step>) {
    if !choices.is_empty() {
        steps.push(Step::Choice(std::mem::take(choices)));
    }
}

fn parse_directive(
    directive: &str,
    line_no: usize,
    script: &mut Script,
) -> Result<(), ScriptError> {
    let mut parts = directive.split_whitespace();
    let name = parts.next().ok_or(err(line_no, "empty directive"))?;
    let args: Vec<&str> = parts.collect();

    match name {
        "scene" => {
            let [id] = one_arg(&args, line_no, "scene")?;
            require_ident(id, line_no)?;
            script.steps.push(Step::Scene(id.to_string()));
        }
        "show" => {
            if args.is_empty() || args.len() > 2 {
                return Err(err(line_no, "`~ show` takes `<cast> [mood]`"));
            }
            let cast_id = args[0];
            require_ident(cast_id, line_no)?;
            let mood = args.get(1).copied().unwrap_or("neutral");
            script.steps.push(Step::Show {
                cast: cast_id.to_string(),
                mood: mood.to_string(),
            });
        }
        "hide" => {
            let [cast_id] = one_arg(&args, line_no, "hide")?;
            require_ident(cast_id, line_no)?;
            script.steps.push(Step::Hide(cast_id.to_string()));
        }
        "label" => {
            let [label] = one_arg(&args, line_no, "label")?;
            require_ident(label, line_no)?;
            if script.labels.contains_key(label) {
                return Err(err(line_no, format!("duplicate label `{label}`")));
            }
            script.labels.insert(label.to_string(), script.steps.len());
            script.steps.push(Step::Label(label.to_string()));
        }
        "jump" => {
            let [target] = one_arg(&args, line_no, "jump")?;
            require_ident(target, line_no)?;
            script.steps.push(Step::Jump(target.to_string()));
        }
        "affinity" => {
            let [delta] = one_arg(&args, line_no, "affinity")?;
            let delta: i32 = delta
                .parse()
                .map_err(|_| err(line_no, format!("`{delta}` is not an integer")))?;
            script.steps.push(Step::Affinity(delta));
        }
        "flag" => {
            let [flag] = one_arg(&args, line_no, "flag")?;
            require_ident(flag, line_no)?;
            script.steps.push(Step::Flag(flag.to_string()));
        }
        "ending" => {
            let [id] = one_arg(&args, line_no, "ending")?;
            require_ident(id, line_no)?;
            script.steps.push(Step::Ending(id.to_string()));
        }
        other => {
            return Err(err(
                line_no,
                format!(
                    "unknown directive `~ {other}` (known: scene, show, hide, label, jump, affinity, flag, ending)"
                ),
            ));
        }
    }
    Ok(())
}

fn one_arg<'a>(
    args: &[&'a str],
    line_no: usize,
    directive: &str,
) -> Result<[&'a str; 1], ScriptError> {
    match args {
        [only] => Ok([only]),
        _ => Err(err(
            line_no,
            format!("`~ {directive}` takes exactly one argument"),
        )),
    }
}

fn require_ident(s: &str, line_no: usize) -> Result<(), ScriptError> {
    let mut chars = s.chars();
    let ok = matches!(chars.next(), Some(first) if first.is_ascii_lowercase() || first == '_')
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if ok {
        Ok(())
    } else {
        Err(err(
            line_no,
            format!("`{s}` must be a lowercase identifier ([a-z_][a-z0-9_]*)"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_directives_and_dialogue() {
        let script = Script::parse(
            "# comment\n\n~ scene lab\n~ show jensen happy\n~ affinity +2\n~ flag x1\njensen: Hello.\nnarrator: It is warm.\n",
        )
        .unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(
            script.steps,
            vec![
                Step::Scene("lab".into()),
                Step::Show {
                    cast: "jensen".into(),
                    mood: "happy".into()
                },
                Step::Affinity(2),
                Step::Flag("x1".into()),
                Step::Say {
                    speaker: "jensen".into(),
                    text: "Hello.".into()
                },
                Step::Say {
                    speaker: "narrator".into(),
                    text: "It is warm.".into()
                },
            ]
        );
    }

    #[test]
    fn groups_consecutive_choices() {
        let script = Script::parse("* A -> a\n* B -> b\njensen: Done.\n")
            .unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(
            script.steps,
            vec![
                Step::Choice(vec![
                    ChoiceOption {
                        text: "A".into(),
                        target: "a".into()
                    },
                    ChoiceOption {
                        text: "B".into(),
                        target: "b".into()
                    },
                ]),
                Step::Say {
                    speaker: "jensen".into(),
                    text: "Done.".into()
                },
            ]
        );
    }

    #[test]
    fn registers_labels_at_own_index() {
        let script = Script::parse("jensen: Hi.\n~ label end\n~ ending done\n")
            .unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(script.label_index("end"), Some(1));
        assert_eq!(script.steps[2], Step::Ending("done".into()));
    }

    #[test]
    fn rejects_unknown_speaker_with_line_number() {
        let Err(e) = Script::parse("sundar: Hi.\n") else {
            panic!("expected unknown-speaker parse error");
        };
        assert_eq!(e.line, 1);
        assert!(e.message.contains("sundar"));
    }

    #[test]
    fn rejects_duplicate_labels() {
        assert!(Script::parse("~ label a\n~ label a\n").is_err());
    }

    #[test]
    fn rejects_choice_without_arrow() {
        let Err(e) = Script::parse("* no arrow here\n") else {
            panic!("expected missing-arrow parse error");
        };
        assert_eq!(e.line, 1);
    }

    #[test]
    fn rejects_negative_identifier_like_labels() {
        assert!(Script::parse("~ label 9bad\n").is_err());
    }
}
