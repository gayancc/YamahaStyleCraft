use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClipboardNote {
    id: u64,
    off_id: u64,
    track: u32,
    channel: u8,
    pitch: u8,
    velocity: u8,
    start: u64,
    duration: u64,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteClipboard {
    pub ppq: u16,
    pub notes: Vec<ClipboardNote>,
}

impl NoteClipboard {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=32767).contains(&self.ppq)
            || self.notes.len() > 50_000
            || self.notes.iter().any(|n| {
                n.id > 9_007_199_254_740_991
                    || n.off_id > 9_007_199_254_740_991
                    || n.track >= 65536
                    || !(1..=16).contains(&n.channel)
                    || n.pitch > 127
                    || !(1..=127).contains(&n.velocity)
                    || n.duration == 0
                    || n.start
                        .checked_add(n.duration)
                        .is_none_or(|end| end > genos_core::smf::MAX_TICK)
            })
        {
            return Err("Invalid note clipboard timing or note data".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn valid() -> NoteClipboard {
        NoteClipboard {
            ppq: 480,
            notes: vec![ClipboardNote {
                id: 1,
                off_id: 2,
                track: 0,
                channel: 9,
                pitch: 60,
                velocity: 90,
                start: 50,
                duration: 70,
            }],
        }
    }
    #[test]
    fn valid_clock_and_notes_are_accepted_without_rewriting_them() {
        let data = valid();
        assert!(data.validate().is_ok());
        assert_eq!(data.notes[0].start, 50);
    }
    #[test]
    fn invalid_clock_overflow_and_non_midi_values_reject() {
        let mut data = valid();
        data.ppq = 0;
        assert!(data.validate().is_err());
        let mut data = valid();
        data.notes[0].start = u64::MAX;
        assert!(data.validate().is_err());
        let mut data = valid();
        data.notes[0].velocity = 128;
        assert!(data.validate().is_err());
        let mut data = valid();
        data.notes[0].duration = 0;
        assert!(data.validate().is_err());
        let mut data = valid();
        data.notes[0].id = 9_007_199_254_740_992;
        assert!(data.validate().is_err());
    }
}
