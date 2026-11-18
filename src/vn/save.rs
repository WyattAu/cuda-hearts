//! Save/load: snapshot a [`Playhead`] to JSON on disk.

use std::collections::BTreeSet;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::vn::state::{Playhead, StageActor};

/// A serializable snapshot of one run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveData {
    pub script_id: String,
    pub pc: usize,
    pub affinity: i64,
    pub flags: BTreeSet<String>,
    pub scene: Option<String>,
    pub stage: Vec<StageActor>,
    /// Oldest first, newest last.
    pub transcript: Vec<String>,
    pub ending: Option<String>,
}

impl Playhead {
    /// Snapshot this run.
    pub fn to_save(&self) -> SaveData {
        SaveData {
            script_id: self.script_id.clone(),
            pc: self.pc,
            affinity: self.affinity,
            flags: self.flags.clone(),
            scene: self.scene.clone(),
            stage: self.stage.clone(),
            transcript: self.transcript.iter().cloned().collect(),
            ending: self.ending.clone(),
        }
    }

    /// Rebuild a run from a snapshot.
    ///
    /// The playhead is validated lazily: an out-of-range `pc` simply plays
    /// as [`crate::vn::state::Flow::Done`]; callers wanting strict
    /// validation should check `pc` against the loaded script first.
    pub fn from_save(data: SaveData) -> Self {
        let mut playhead = Playhead::start(data.script_id);
        playhead.pc = data.pc;
        playhead.affinity = data.affinity;
        playhead.flags = data.flags;
        playhead.scene = data.scene;
        playhead.stage = data.stage;
        playhead.transcript = data.transcript.into();
        playhead.ending = data.ending;
        playhead
    }
}

/// Write a save as pretty JSON, creating parent directories as needed.
pub fn save_to_path(path: &Path, data: &SaveData) -> io::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(data)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, json)
}

/// Read a save written by [`save_to_path`].
pub fn load_from_path(path: &Path) -> io::Result<SaveData> {
    let json = std::fs::read_to_string(path)?;
    serde_json::from_str(&json).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[test]
    fn round_trips_through_memory() {
        let mut p = Playhead::start("prologue");
        p.pc = 7;
        p.affinity = -3;
        p.scene = Some("rooftop".into());
        p.stage.push(StageActor {
            cast: "jensen".into(),
            mood: "neutral".into(),
        });
        p.transcript = VecDeque::from(["jensen: Hi.".to_string()]);

        let restored = Playhead::from_save(p.to_save());
        assert_eq!(restored, p);
    }

    #[test]
    fn round_trips_through_disk() {
        let mut p = Playhead::start("prologue");
        p.affinity = 2;
        let data = p.to_save();

        let path =
            std::env::temp_dir().join(format!("cuda-hearts-test-{}.json", std::process::id()));
        save_to_path(&path, &data).unwrap_or_else(|e| panic!("save failed: {e}"));
        let loaded = load_from_path(&path).unwrap_or_else(|e| panic!("load failed: {e}"));
        std::fs::remove_file(&path).ok();
        assert_eq!(loaded, data);
    }
}
