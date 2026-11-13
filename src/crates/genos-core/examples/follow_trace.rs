//! Offline diagnostic: prints each source note of one channel in a section with the
//! pitch the live chord follow produces and the CASM route that converted it.
//!
//! Usage: follow_trace <sty> <section-label> <channel 1-16> <root 0-11> <quality>
use genos_core::{document::Document, live_preview, midi};
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 5 {
        return Err("usage: follow_trace <sty> <section-label> <channel> <root> <quality>".into());
    }
    let doc = Document::parse(&fs::read(&args[0])?)?;
    let section = doc
        .sections
        .iter()
        .find(|s| s.label == args[1])
        .ok_or("section not found")?;
    let channel: u8 = args[2].parse()?;
    let chord = midi::LiveChord {
        root: args[3].parse()?,
        quality: args[4].clone(),
        bass: None,
        bass_quality: None,
    };
    for route in doc
        .routes
        .iter()
        .filter(|r| r.source == channel && r.sections.contains(&section.label))
    {
        println!(
            "route source {} -> destination {} bytes {:02X?}",
            route.source, route.destination, route.bytes
        );
    }
    let plan = midi::plan(&doc, section.id, &[channel])?;
    for packet in &plan.packets {
        if packet.data.len() == 3 && packet.data[0] & 0xf0 == 0x90 && packet.data[2] > 0 {
            let source = packet.data[1];
            let mapped =
                live_preview::follow_routed_pitch(&doc, section.id, channel, source, &chord, false);
            println!(
                "{:>7.1} ms  source {:>3}  ->  {:?}  bassRegion {}",
                packet.at.as_secs_f64() * 1000.0,
                source,
                mapped.pitch,
                mapped.bass_region
            );
        }
    }
    Ok(())
}
