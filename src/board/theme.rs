//! Board color themes. The choice is saved in the `settings` table under `SETTING_KEY`.

use ratatui::style::Color;

pub const SETTING_KEY: &str = "board.theme";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeName {
    #[default]
    CatppuccinMocha,
    TokyoNight,
    Nord,
}

impl ThemeName {
    const ALL: [ThemeName; 3] = [
        ThemeName::CatppuccinMocha,
        ThemeName::TokyoNight,
        ThemeName::Nord,
    ];

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ThemeName::CatppuccinMocha => "catppuccin-mocha",
            ThemeName::TokyoNight => "tokyo-night",
            ThemeName::Nord => "nord",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.as_str() == s.trim())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub border: Color,
    pub title: Color,
    pub selected: Color,
    pub ok: Color,
    pub working: Color,
    pub warn: Color,
    pub error: Color,
    pub muted: Color,
    pub accent: Color,
}

const fn hex(v: u32) -> Color {
    Color::Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub fn theme(name: ThemeName) -> Theme {
    let [
        border,
        title,
        selected,
        ok,
        working,
        warn,
        error,
        muted,
        accent,
    ] = match name {
        ThemeName::CatppuccinMocha => [
            0x45475a, 0xb4befe, 0xcba6f7, 0xa6e3a1, 0x89b4fa, 0xf9e2af, 0xf38ba8, 0x6c7086,
            0xfab387,
        ],
        ThemeName::TokyoNight => [
            0x3b4261, 0x7aa2f7, 0xbb9af7, 0x9ece6a, 0x7dcfff, 0xe0af68, 0xf7768e, 0x565f89,
            0xff9e64,
        ],
        ThemeName::Nord => [
            0x4c566a, 0x88c0d0, 0x81a1c1, 0xa3be8c, 0x5e81ac, 0xebcb8b, 0xbf616a, 0x616e88,
            0xb48ead,
        ],
    };
    Theme {
        border: hex(border),
        title: hex(title),
        selected: hex(selected),
        ok: hex(ok),
        working: hex(working),
        warn: hex(warn),
        error: hex(error),
        muted: hex(muted),
        accent: hex(accent),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_cycle_wraps() {
        assert_eq!(ThemeName::CatppuccinMocha.next(), ThemeName::TokyoNight);
        assert_eq!(ThemeName::TokyoNight.next(), ThemeName::Nord);
        assert_eq!(ThemeName::Nord.next(), ThemeName::CatppuccinMocha);
    }

    #[test]
    fn theme_parse_roundtrip() {
        for t in ThemeName::ALL {
            assert_eq!(ThemeName::parse(t.as_str()), Some(t));
        }
        assert_eq!(ThemeName::parse("x"), None);
        assert_eq!(theme(ThemeName::Nord).ok, Color::Rgb(0xa3, 0xbe, 0x8c));
    }
}
