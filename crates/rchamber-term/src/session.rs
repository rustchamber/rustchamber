use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::emulator::Emulator;

#[derive(Debug, thiserror::Error)]
pub enum TermError {
    #[error("{0}")]
    Message(String),
}

pub type Wake = Arc<dyn Fn() + Send + Sync + 'static>;

pub struct Terminal {
    pub title: String,
    pub cwd: PathBuf,
    pub emulator: Emulator,
    master: Option<Box<dyn MasterPty + Send>>,
    writer: Option<Box<dyn Write + Send>>,
    child: Option<Box<dyn Child + Send + Sync>>,
    output: Arc<Mutex<Vec<u8>>>,
    closed: Arc<AtomicBool>,
    cols: usize,
    rows: usize,
    exited: Option<u32>,
    killed: bool,
}

impl Terminal {
    pub fn spawn(cwd: &Path, title: impl Into<String>) -> Result<Self, TermError> {
        Self::spawn_with(cwd, title, 80, 24, Arc::new(|| {}))
    }

    pub fn spawn_with(
        cwd: &Path,
        title: impl Into<String>,
        cols: usize,
        rows: usize,
        wake: Wake,
    ) -> Result<Self, TermError> {
        let cols = cols.max(20);
        let rows = rows.max(5);
        let size = PtySize {
            rows: rows.min(u16::MAX as usize) as u16,
            cols: cols.min(u16::MAX as usize) as u16,
            pixel_width: 0,
            pixel_height: 0,
        };
        let pair = native_pty_system()
            .openpty(size)
            .map_err(|error| TermError::Message(error.to_string()))?;
        let mut command = CommandBuilder::new(shell());
        command.cwd(cwd);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        command.env("TERM_PROGRAM", "RustChamber");
        command.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| TermError::Message(error.to_string()))?;
        drop(pair.slave);
        let master = pair.master;
        let writer = master
            .take_writer()
            .map_err(|error| TermError::Message(error.to_string()))?;
        let mut reader = master
            .try_clone_reader()
            .map_err(|error| TermError::Message(error.to_string()))?;
        let output = Arc::new(Mutex::new(Vec::new()));
        let closed = Arc::new(AtomicBool::new(false));
        let thread_output = output.clone();
        let thread_closed = closed.clone();
        let thread_wake = wake.clone();
        std::thread::Builder::new()
            .name("rchamber-pty-reader".into())
            .spawn(move || {
                let mut buffer = [0u8; 16384];
                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(count) => {
                            let mut queue = thread_output
                                .lock()
                                .unwrap_or_else(|error| error.into_inner());
                            queue.extend_from_slice(&buffer[..count]);
                            drop(queue);
                            thread_wake();
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(_) => break,
                    }
                }
                thread_closed.store(true, Ordering::SeqCst);
                thread_wake();
            })
            .map_err(|error| TermError::Message(error.to_string()))?;
        Ok(Self {
            title: title.into(),
            cwd: cwd.to_path_buf(),
            emulator: Emulator::new(cols, rows),
            master: Some(master),
            writer: Some(writer),
            child: Some(child),
            output,
            closed,
            cols,
            rows,
            exited: None,
            killed: false,
        })
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<(), TermError> {
        self.write_raw(bytes)?;
        self.emulator.scroll_to_bottom();
        Ok(())
    }

    fn write_raw(&mut self, bytes: &[u8]) -> Result<(), TermError> {
        let Some(writer) = self.writer.as_mut() else {
            return Ok(());
        };
        writer
            .write_all(bytes)
            .and_then(|_| writer.flush())
            .map_err(|error| TermError::Message(error.to_string()))
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.max(20);
        let rows = rows.max(5);
        if cols == self.cols && rows == self.rows {
            return;
        }
        self.cols = cols;
        self.rows = rows;
        self.emulator.resize(cols, rows);
        if let Some(master) = self.master.as_ref() {
            let _ = master.resize(PtySize {
                rows: rows.min(u16::MAX as usize) as u16,
                cols: cols.min(u16::MAX as usize) as u16,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }

    pub fn kill(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
        }
        self.child = None;
        self.writer = None;
        self.killed = true;
    }

    pub fn pump(&mut self) -> bool {
        let data = {
            let mut queue = self
                .output
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if queue.is_empty() {
                None
            } else {
                Some(std::mem::take(&mut *queue))
            }
        };
        let mut changed = false;
        if let Some(data) = data {
            self.emulator.feed(&data);
            let responses = self.emulator.take_responses();
            if !responses.is_empty() {
                let _ = self.write_raw(&responses);
            }
            changed = true;
        }
        if self.exited.is_none()
            && !self.killed
            && let Some(child) = self.child.as_mut()
            && let Ok(Some(status)) = child.try_wait()
        {
            self.exited = Some(status.exit_code());
        }
        changed
    }

    pub fn is_running(&self) -> bool {
        !self.killed && self.exited.is_none() && self.child.is_some()
    }

    pub fn exit_code(&self) -> Option<u32> {
        self.exited
    }

    pub fn killed(&self) -> bool {
        self.killed
    }

    pub fn reader_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn rows(&self) -> usize {
        self.rows
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
        }
    }
}

fn shell() -> String {
    if let Ok(value) = std::env::var("SHELL") {
        let value = value.trim();
        if !value.is_empty() {
            return value.to_string();
        }
    }
    if cfg!(target_os = "windows") {
        "powershell.exe".into()
    } else if cfg!(target_os = "macos") {
        "/bin/zsh".into()
    } else {
        "/bin/bash".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn screen_text(terminal: &Terminal) -> String {
        let emulator = &terminal.emulator;
        (0..emulator.total_lines())
            .filter_map(|index| emulator.line(index))
            .map(|cells| cells.iter().map(|cell| cell.ch).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    #[ignore]
    fn pty_round_trip() {
        let cwd = std::env::current_dir().unwrap();
        let mut terminal = Terminal::spawn_with(&cwd, "test", 80, 24, Arc::new(|| {})).unwrap();
        terminal
            .write(b"printf 'rchamber_pty_ok\\n' | tr '[:lower:]' '[:upper:]'\r")
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            terminal.pump();
            if screen_text(&terminal).contains("RCHAMBER_PTY_OK") {
                break;
            }
            assert!(Instant::now() < deadline, "shell produced no output");
            std::thread::sleep(Duration::from_millis(40));
        }
        terminal
            .write(b"printf '\\033[31mRCHAMBER_RED\\033[0m\\n'\r")
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            terminal.pump();
            let emulator = &terminal.emulator;
            let colored = (0..emulator.total_lines()).any(|index| {
                let Some(cells) = emulator.line(index) else {
                    return false;
                };
                cells.windows(12).any(|window| {
                    window.iter().map(|cell| cell.ch).collect::<String>() == "RCHAMBER_RED"
                        && window[0].fg == crate::emulator::Color::Indexed(1)
                })
            });
            if colored {
                break;
            }
            assert!(Instant::now() < deadline, "shell produced no color output");
            std::thread::sleep(Duration::from_millis(40));
        }
        terminal.resize(100, 30);
        terminal.kill();
    }
}

#[cfg(test)]
mod vim_tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn screen_text(terminal: &Terminal) -> String {
        let emulator = &terminal.emulator;
        (0..emulator.total_lines())
            .filter_map(|index| emulator.line(index))
            .map(|cells| cells.iter().map(|cell| cell.ch).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    #[ignore]
    fn vim_uses_alternate_screen() {
        let cwd = std::env::current_dir().unwrap();
        let mut terminal = Terminal::spawn_with(&cwd, "test", 80, 24, Arc::new(|| {})).unwrap();
        terminal.write(b"vim -u NONE\r").unwrap();
        let deadline = Instant::now() + Duration::from_secs(8);
        while !terminal.emulator.is_alt_screen() {
            terminal.pump();
            assert!(
                Instant::now() < deadline,
                "vim did not enter the alternate screen: {}",
                screen_text(&terminal)
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        terminal.pump();
        assert!(screen_text(&terminal).contains("VIM"));
        terminal.write(b"\x1b:q!\r").unwrap();
        let deadline = Instant::now() + Duration::from_secs(8);
        while terminal.emulator.is_alt_screen() {
            terminal.pump();
            assert!(
                Instant::now() < deadline,
                "vim did not leave the alternate screen: {}",
                screen_text(&terminal)
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        terminal.kill();
    }
}
