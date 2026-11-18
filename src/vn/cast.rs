//! The cast: a static character registry shared by the parser and renderer.

/// Speaker id for narration lines (rendered without a name plate).
pub const NARRATOR: &str = "narrator";

/// Speaker id for the player character.
pub const YOU: &str = "you";

/// One character definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Character {
    /// Id used in scripts (`jensen: ...`, `~ show jensen happy`).
    pub id: &'static str,
    /// Display name shown on the name plate.
    pub name: &'static str,
    /// sRGB hex color for the name plate and placeholder stage chips.
    pub color: &'static str,
}

/// The full cast. Fictional characters are parody figures; see README.
pub const CAST: &[Character] = &[
    Character {
        id: "jensen",
        name: "Jensen",
        color: "#76B900", // NVIDIA green
    },
    Character {
        id: "you",
        name: "You",
        color: "#7EC8E3",
    },
    Character {
        id: "ava",
        name: "AVA",
        color: "#C77DFF",
    },
    Character {
        id: "crowd",
        name: "The Crowd",
        color: "#B0BEC5",
    },
];

/// Look up a character by id.
pub fn character(id: &str) -> Option<&'static Character> {
    CAST.iter().find(|c| c.id == id)
}

/// Whether a script may attribute a dialogue line to this speaker id.
pub fn is_known_speaker(id: &str) -> bool {
    id == NARRATOR || character(id).is_some()
}

impl Character {
    /// The character's color as `(r, g, b)` bytes, or a neutral gray when
    /// the hex is malformed.
    pub fn rgb(&self) -> (u8, u8, u8) {
        parse_hex(self.color).unwrap_or((0xB0, 0xB0, 0xB0))
    }
}

/// Parse `#RRGGBB` into `(r, g, b)`.
fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> {
    let digits = hex.strip_prefix('#')?;
    if digits.len() != 6 {
        return None;
    }
    let value = u32::from_str_radix(digits, 16).ok()?;
    Some(((value >> 16) as u8, (value >> 8) as u8, value as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jensen_is_registered() {
        let jensen = character("jensen").unwrap_or_else(|| panic!("jensen missing from CAST"));
        assert_eq!(jensen.name, "Jensen");
        assert_eq!(jensen.rgb(), (0x76, 0xB9, 0x00));
    }

    #[test]
    fn narrator_is_a_speaker_but_not_a_character() {
        assert!(is_known_speaker(NARRATOR));
        assert!(character(NARRATOR).is_none());
    }

    #[test]
    fn unknown_ids_are_rejected() {
        assert!(!is_known_speaker("sundar"));
        assert!(character("nobody").is_none());
    }

    #[test]
    fn bad_hex_falls_back_to_gray() {
        let c = Character {
            id: "x",
            name: "X",
            color: "#nope",
        };
        assert_eq!(c.rgb(), (0xB0, 0xB0, 0xB0));
    }
}
