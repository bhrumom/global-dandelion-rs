//! Read-only latest-value terminal observer. Runs on an owned blocking worker.
use dandelion_core::{Error, Result, Snapshot};
use std::{
    io::{self, IsTerminal, Write},
    time::{Duration, Instant},
};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
pub fn check_terminal() -> Result<()> {
    if !io::stderr().is_terminal() || std::env::var("TERM").is_ok_and(|s| s == "dumb") {
        return Err(Error::Config(
            "--tui requires an interactive ANSI terminal on stderr".into(),
        ));
    }
    Ok(())
}
struct Screen<W: Write> {
    writer: W,
}
impl<W: Write> Screen<W> {
    fn enter(mut writer: W) -> Result<Self> {
        writer.write_all(b"\x1b[?1049h\x1b[?25l")?;
        writer.flush()?;
        Ok(Self { writer })
    }
}
impl<W: Write> Drop for Screen<W> {
    fn drop(&mut self) {
        let _ = self.writer.write_all(b"\x1b[?25h\x1b[?1049l");
        let _ = self.writer.flush();
    }
}
pub fn render(snapshot: &Snapshot, elapsed: Duration) -> String {
    format!(
        "Global Dandelion | {:?}\n\nFiles completed: {}\nChunks verified: {}\nAttempted wire bytes: {}\nAccepted wire bytes: {}\nVerified payload bytes: {}\nElapsed: {:.1}s\n\nCtrl+C to cancel. Source files are read-only.\n",
        snapshot.status,
        snapshot.files_completed,
        snapshot.chunks_sent,
        snapshot.attempted_wire_bytes,
        snapshot.accepted_wire_bytes,
        snapshot.verified_payload_bytes,
        elapsed.as_secs_f64()
    )
}
pub fn observe(receiver: watch::Receiver<Snapshot>, cancel: CancellationToken) -> Result<()> {
    let mut screen = Screen::enter(io::stderr())?;
    let start = Instant::now();
    while !cancel.is_cancelled() {
        let value = receiver.borrow().clone();
        screen.writer.write_all(b"\x1b[H\x1b[2J")?;
        screen
            .writer
            .write_all(render(&value, start.elapsed()).as_bytes())?;
        screen.writer.flush()?;
        // Explicit four-Hz UI refresh, wholly independent of data-plane pacing.
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terminal_restored_on_early_return() {
        let mut bytes = Vec::new();
        {
            let _screen = Screen::enter(&mut bytes).unwrap();
        }
        assert!(bytes.ends_with(b"\x1b[?25h\x1b[?1049l"));
    }
    #[test]
    fn snapshot_render_is_pure() {
        let s = Snapshot {
            verified_payload_bytes: 42,
            ..Default::default()
        };
        assert!(render(&s, Duration::ZERO).contains("42"));
        assert_eq!(s.verified_payload_bytes, 42);
    }
}
