use std::time::{Duration, Instant};
fn main() {
    let st = Instant::now();
    let mut last = String::new();
    while st.elapsed() < Duration::from_secs(40) {
        let o = midir::MidiOutput::new("poll").unwrap();
        let ports = o.ports();
        let r = match ports.get(2) {
            Some(p) => match o.connect(p, "poll") {
                Ok(c) => {
                    drop(c);
                    "free".to_string()
                }
                Err(e) => format!("{e}"),
            },
            None => "noport".into(),
        };
        if r != last {
            println!("{:>6}ms {r}", st.elapsed().as_millis());
            last = r;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}
