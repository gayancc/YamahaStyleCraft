use genos_core::{document::Document, midi};
use std::time::{Duration, Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let style = std::env::args_os().nth(1).ok_or("usage: zz_play_repro <style-file>")?;
    let bytes = std::fs::read(style)?;
    let doc = Document::parse(&bytes)?;
    let s = doc.sections.iter().find(|s| s.label == "Main A").unwrap();
    let mut chans: Vec<u8> = doc
        .notes
        .iter()
        .filter(|n| n.start >= s.start && n.start < s.end)
        .map(|n| n.channel)
        .collect();
    chans.sort_unstable();
    chans.dedup();
    let dev = midi::devices()?;
    let p = dev.outputs.iter().find(|p| p.name == "Genos OUT").unwrap();
    let mut t = midi::Transport::default();
    let r = t.play_with_initial_chord(
        &doc,
        midi::PlaybackOptions {
            section: s.id,
            channels: chans.clone(),
            index: p.index,
            name: p.name.clone(),
            looping: true,
            initial_effects: false,
            from_tick: Some(27871),
        },
        Some(midi::LiveChord {
            root: 0,
            quality: "Genos:34".into(),
            bass: None,
            bass_quality: None,
        }),
    );
    println!("play result: {r:?} channels {chans:?}");
    let st = Instant::now();
    while st.elapsed() < Duration::from_secs(6) {
        let s = t.status();
        println!(
            "{:>5}ms playing={} elapsed={} err={:?}",
            st.elapsed().as_millis(),
            s.playing,
            s.elapsed_ms,
            s.error
        );
        std::thread::sleep(Duration::from_millis(1000));
    }
    t.stop();
    println!("final: {:?}", t.status().error);
    Ok(())
}
