//! Running pacman and friends inside a pseudo-terminal, so sudo can ask for a
//! password and the output streams into the interface line by line.

use std::io::{Read, Write};
use std::sync::mpsc::Sender;
use std::thread;

use anyhow::{Context, Result};
use portable_pty::{ChildKiller, CommandBuilder, PtySize, native_pty_system};

use crate::queue::Step;

/// What the runner reports back.
#[derive(Debug)]
pub enum ExecEvent {
    /// A completed line, ANSI stripped.
    Line(String),
    /// The current, unfinished line (progress bars redraw it).
    Partial(String),
    /// The process ended with this exit code.
    Done(i32),
}

pub struct Runner {
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    master: Box<dyn portable_pty::MasterPty + Send>,
}

impl Runner {
    /// Start `step` in a pty of the given size. Events go to `tx` from background threads.
    pub fn spawn(step: &Step, rows: u16, cols: u16, tx: Sender<ExecEvent>) -> Result<Self> {
        let pty = native_pty_system();
        let pair = pty.openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }).context("opening a pty")?;
        let mut cmd = CommandBuilder::new(&step.program);
        cmd.args(&step.args);
        cmd.env("TERM", "xterm-256color");
        cmd.env("LANG", std::env::var("LANG").unwrap_or_else(|_| "C.UTF-8".into()));
        let mut child = pair.slave.spawn_command(cmd).with_context(|| format!("starting {}", step.program))?;
        drop(pair.slave);
        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader().context("pty reader")?;
        let writer = pair.master.take_writer().context("pty writer")?;

        let tx_out = tx.clone();
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            let mut line = String::new();
            // A '\r' was seen: the next char either ends the line ("\r\n", what a
            // pty writes for every newline) or redraws it (a progress bar).
            let mut cr = false;
            let mut pending = Vec::new();
            loop {
                let n = match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                pending.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&pending).into_owned();
                // Keep an incomplete UTF-8 tail for the next read.
                let valid_up_to = match std::str::from_utf8(&pending) {
                    Ok(_) => pending.len(),
                    Err(e) => e.valid_up_to(),
                };
                let text: String = if valid_up_to == pending.len() {
                    text
                } else {
                    String::from_utf8_lossy(&pending[..valid_up_to]).into_owned()
                };
                pending.drain(..valid_up_to);
                for ch in strip_ansi(&text).chars() {
                    match ch {
                        '\n' => {
                            cr = false;
                            let _ = tx_out.send(ExecEvent::Line(std::mem::take(&mut line)));
                        }
                        '\r' => {
                            cr = true;
                            if !line.is_empty() {
                                let _ = tx_out.send(ExecEvent::Partial(line.clone()));
                            }
                        }
                        c if c.is_control() && c != '\t' => {}
                        c => {
                            if cr {
                                // A redraw, not a line end: start the line over.
                                line.clear();
                                cr = false;
                            }
                            line.push(c);
                        }
                    }
                }
                if !line.is_empty() {
                    let _ = tx_out.send(ExecEvent::Partial(line.clone()));
                }
            }
            if !line.is_empty() {
                let _ = tx_out.send(ExecEvent::Line(line));
            }
        });

        thread::spawn(move || {
            let code = match child.wait() {
                Ok(status) => status.exit_code() as i32,
                Err(_) => -1,
            };
            let _ = tx.send(ExecEvent::Done(code));
        });

        Ok(Self { writer, killer, master: pair.master })
    }

    /// Forward keystrokes (a sudo password, an answer to a prompt).
    pub fn write(&mut self, bytes: &[u8]) {
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
    }

    pub fn resize(&self, rows: u16, cols: u16) {
        let _ = self.master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
    }

    pub fn kill(&mut self) {
        let _ = self.killer.kill();
    }
}

/// Remove ANSI escape sequences (colors, cursor movement, OSC titles).
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                // CSI: parameters and intermediates, then a final byte 0x40..=0x7E.
                for c in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                // OSC: until BEL or ESC \.
                let mut prev = '\0';
                for c in chars.by_ref() {
                    if c == '\x07' || (prev == '\x1b' && c == '\\') {
                        break;
                    }
                    prev = c;
                }
            }
            Some(_) | None => {}
        }
    }
    out
}

/// Run a list of steps on the real terminal (the command line path).
pub fn run_inherit(steps: &[Step]) -> Result<()> {
    for step in steps {
        eprintln!("\n▸ {}\n  $ {}", step.title, step.command_line());
        let status = std::process::Command::new(&step.program)
            .args(&step.args)
            .status()
            .with_context(|| format!("running {}", step.program))?;
        if !status.success() {
            anyhow::bail!("{} failed with {}", step.program, status);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn strips_colors_and_titles() {
        assert_eq!(
            super::strip_ansi("\x1b[1;32m:: \x1b[0mSynchronizing\x1b]0;title\x07 done"),
            ":: Synchronizing done"
        );
    }
}
