//! Press-and-hold key audition (piano-roll keyboard).
//!
//! Unlike `midi::test_note`, which blocks for a fixed hold time, this keeps a
//! note sounding between `note_on` and `note_off`. The output connection is
//! opened on the first held note and dropped as soon as nothing is held, so the
//! (exclusive, WinMM) port is not occupied while the user is not playing keys.
use crate::{Error, Result, voice_catalog};
use midir::{MidiOutput, MidiOutputConnection};
use std::collections::BTreeSet;

/// Destination for raw MIDI bytes; lets the hold logic be tested without a port.
pub trait NoteSink {
    fn send(&mut self, data: &[u8]) -> Result<()>;
}

impl NoteSink for MidiOutputConnection {
    fn send(&mut self, data: &[u8]) -> Result<()> {
        MidiOutputConnection::send(self, data).map_err(|e| Error::Midi(e.to_string()))
    }
}

/// Validate an audition note. Drum channels follow `preview_drum_note` bounds.
pub fn validate(channel: u8, pitch: u8, velocity: u8) -> Result<()> {
    if !(1..=16).contains(&channel) {
        return Err(Error::Midi("channel must be 1..16".into()));
    }
    if pitch > 127 || velocity == 0 || velocity > 127 {
        return Err(Error::Midi("pitch/velocity out of range".into()));
    }
    if voice_catalog::DRUM_CHANNELS.contains(&channel)
        && !(voice_catalog::DRUM_PREVIEW_NOTE_MIN..=voice_catalog::DRUM_PREVIEW_NOTE_MAX)
            .contains(&pitch)
    {
        return Err(Error::Midi(
            "drum preview notes are limited to 13..91 for Genos shell bounds".into(),
        ));
    }
    Ok(())
}

/// Notes currently held, with idempotent on/off so a glissando or a duplicate
/// event can never leave a note sounding or double-trigger it.
#[derive(Default, Debug)]
pub struct HeldNotes {
    held: BTreeSet<(u8, u8)>,
}

impl HeldNotes {
    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }
    pub fn len(&self) -> usize {
        self.held.len()
    }
    pub fn note_on(
        &mut self,
        sink: &mut dyn NoteSink,
        channel: u8,
        pitch: u8,
        velocity: u8,
    ) -> Result<()> {
        validate(channel, pitch, velocity)?;
        let status = channel - 1;
        if self.held.contains(&(channel, pitch)) {
            // Retrigger cleanly instead of stacking a second voice.
            sink.send(&[0x80 | status, pitch, 0])?;
        }
        sink.send(&[0x90 | status, pitch, velocity])?;
        self.held.insert((channel, pitch));
        Ok(())
    }
    pub fn note_off(&mut self, sink: &mut dyn NoteSink, channel: u8, pitch: u8) -> Result<()> {
        if !(1..=16).contains(&channel) || pitch > 127 {
            return Err(Error::Midi("channel/pitch out of range".into()));
        }
        // Always send, even if unknown: a stray note-off is harmless and a
        // missed one is a stuck note.
        self.held.remove(&(channel, pitch));
        sink.send(&[0x80 | (channel - 1), pitch, 0])
    }
    /// Release everything held. Keeps going past send errors; reports the first.
    pub fn release_all(&mut self, sink: &mut dyn NoteSink) -> Result<()> {
        let mut first = None;
        for (channel, pitch) in std::mem::take(&mut self.held) {
            if let Err(e) = sink.send(&[0x80 | (channel - 1), pitch, 0]) {
                first.get_or_insert(e);
            }
        }
        first.map_or(Ok(()), Err)
    }
}

struct Open {
    index: usize,
    name: String,
    connection: MidiOutputConnection,
}

/// Held-note audition bound to one exact output port.
#[derive(Default)]
pub struct KeyAudition {
    open: Option<Open>,
    held: HeldNotes,
}

fn connect(index: usize, name: &str) -> Result<MidiOutputConnection> {
    let output =
        MidiOutput::new("GenosStyleStudio key audition").map_err(|e| Error::Midi(e.to_string()))?;
    let ports = output.ports();
    let port = ports
        .get(index)
        .ok_or_else(|| Error::Midi("output disconnected; refresh devices".into()))?;
    if output
        .port_name(port)
        .map_err(|e| Error::Midi(e.to_string()))?
        != name
    {
        return Err(Error::Midi(
            "device list changed; select the output again".into(),
        ));
    }
    output
        .connect(port, "GenosStyleStudio key audition")
        .map_err(|e| Error::Midi(e.to_string()))
}

impl KeyAudition {
    pub fn note_on(
        &mut self,
        index: usize,
        name: &str,
        channel: u8,
        pitch: u8,
        velocity: u8,
    ) -> Result<()> {
        validate(channel, pitch, velocity)?;
        if self
            .open
            .as_ref()
            .is_some_and(|o| o.index != index || o.name != name)
        {
            self.release_all();
        }
        if self.open.is_none() {
            self.open = Some(Open {
                index,
                name: name.to_string(),
                connection: connect(index, name)?,
            });
        }
        let open = self.open.as_mut().expect("opened above");
        let result = self
            .held
            .note_on(&mut open.connection, channel, pitch, velocity);
        if result.is_err() || self.held.is_empty() {
            self.open = None;
        }
        result
    }
    pub fn note_off(&mut self, channel: u8, pitch: u8) -> Result<()> {
        let Some(open) = self.open.as_mut() else {
            return Ok(());
        };
        let result = self.held.note_off(&mut open.connection, channel, pitch);
        if result.is_err() || self.held.is_empty() {
            self.open = None;
        }
        result
    }
    /// Silence every held note and give the port back. Never fails loudly.
    pub fn release_all(&mut self) {
        if let Some(open) = self.open.as_mut() {
            let _ = self.held.release_all(&mut open.connection);
        }
        self.held = HeldNotes::default();
        self.open = None;
    }
    pub fn held_count(&self) -> usize {
        self.held.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Log(Vec<Vec<u8>>);
    impl NoteSink for Log {
        fn send(&mut self, data: &[u8]) -> Result<()> {
            self.0.push(data.to_vec());
            Ok(())
        }
    }

    #[test]
    fn hold_sends_on_then_off_on_the_assigned_channel() {
        let (mut h, mut s) = (HeldNotes::default(), Log::default());
        h.note_on(&mut s, 4, 60, 90).unwrap();
        assert_eq!(s.0, vec![vec![0x93, 60, 90]]);
        assert_eq!(h.len(), 1);
        h.note_off(&mut s, 4, 60).unwrap();
        assert_eq!(s.0[1], vec![0x83, 60, 0]);
        assert!(h.is_empty());
    }

    #[test]
    fn retrigger_never_stacks_voices() {
        let (mut h, mut s) = (HeldNotes::default(), Log::default());
        h.note_on(&mut s, 1, 60, 80).unwrap();
        h.note_on(&mut s, 1, 60, 80).unwrap();
        assert_eq!(s.0[1], vec![0x80, 60, 0]);
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn release_all_silences_every_held_note() {
        let (mut h, mut s) = (HeldNotes::default(), Log::default());
        h.note_on(&mut s, 1, 60, 80).unwrap();
        h.note_on(&mut s, 3, 64, 80).unwrap();
        s.0.clear();
        h.release_all(&mut s).unwrap();
        assert_eq!(s.0.len(), 2);
        assert!(s.0.contains(&vec![0x80, 60, 0]) && s.0.contains(&vec![0x82, 64, 0]));
        assert!(h.is_empty());
    }

    #[test]
    fn stray_note_off_is_still_sent() {
        let (mut h, mut s) = (HeldNotes::default(), Log::default());
        h.note_off(&mut s, 2, 70).unwrap();
        assert_eq!(s.0, vec![vec![0x81, 70, 0]]);
    }

    #[test]
    fn validation_matches_test_note_and_drum_bounds() {
        assert!(validate(0, 60, 80).is_err());
        assert!(validate(17, 60, 80).is_err());
        assert!(validate(1, 128, 80).is_err());
        assert!(validate(1, 60, 0).is_err());
        assert!(validate(1, 60, 127).is_ok());
        assert!(validate(10, 12, 80).is_err());
        assert!(validate(10, 92, 80).is_err());
        assert!(validate(10, 36, 80).is_ok());
        assert!(validate(9, 13, 80).is_ok());
        assert!(validate(11, 100, 80).is_ok());
        let (mut h, mut s) = (HeldNotes::default(), Log::default());
        assert!(h.note_on(&mut s, 10, 5, 80).is_err());
        assert!(s.0.is_empty() && h.is_empty());
    }
}
