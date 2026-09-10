//! Colors and marks. Moriya green and lake blue by default; Reimu keeps the red.

use ratatui::style::{Color, Modifier, Style};

use crate::config::ThemeConfig;

#[derive(Clone, Debug)]
pub struct Theme {
    pub accent: Color,
    pub accent2: Color,
    pub dim: Color,
    pub warn: Color,
    pub bad: Color,
    pub ok: Color,
    pub nerd: bool,
}

impl Theme {
    pub fn from_config(c: &ThemeConfig) -> Self {
        Self {
            accent: hex(&c.accent).unwrap_or(Color::Rgb(0x5f, 0xd7, 0xa7)),
            accent2: hex(&c.accent2).unwrap_or(Color::Rgb(0x87, 0xaf, 0xff)),
            dim: Color::DarkGray,
            warn: Color::Yellow,
            bad: Color::Red,
            ok: Color::Green,
            nerd: c.nerd_font,
        }
    }

    pub fn title(&self) -> Style {
        Style::new().fg(self.accent).add_modifier(Modifier::BOLD)
    }
    pub fn border(&self, focused: bool) -> Style {
        if focused { Style::new().fg(self.accent) } else { Style::new().fg(self.dim) }
    }
    pub fn highlight(&self) -> Style {
        Style::new().fg(Color::Black).bg(self.accent).add_modifier(Modifier::BOLD)
    }
    pub fn dim(&self) -> Style {
        Style::new().fg(self.dim)
    }
    pub fn accent(&self) -> Style {
        Style::new().fg(self.accent)
    }
    pub fn accent2(&self) -> Style {
        Style::new().fg(self.accent2)
    }
    pub fn key(&self) -> Style {
        Style::new().fg(self.accent2).add_modifier(Modifier::BOLD)
    }
    pub fn ok(&self) -> Style {
        Style::new().fg(self.ok)
    }
    pub fn warn(&self) -> Style {
        Style::new().fg(self.warn)
    }
    pub fn bad(&self) -> Style {
        Style::new().fg(self.bad).add_modifier(Modifier::BOLD)
    }

    // Marks.
    pub fn installed(&self) -> &'static str {
        if self.nerd { " " } else { "✔" }
    }
    pub fn queued_install(&self) -> &'static str {
        if self.nerd { " " } else { "+" }
    }
    pub fn queued_remove(&self) -> &'static str {
        if self.nerd { " " } else { "−" }
    }
    pub fn update(&self) -> &'static str {
        if self.nerd { " " } else { "↑" }
    }
}

fn hex(s: &str) -> Option<Color> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some(Color::Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}
