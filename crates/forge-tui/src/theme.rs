use ratatui::style::Color;

/// A color theme for the TUI
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: &'static str,
    /// Status bar background
    pub status_bg: Color,
    /// Status bar text
    pub status_fg: Color,
    /// Main background (terminal default)
    pub bg: Color,
    /// Primary text
    pub fg: Color,
    /// User message label
    pub user: Color,
    /// Assistant streaming cursor
    pub assistant: Color,
    /// Tool name in brackets
    pub tool: Color,
    /// Success/complete indicators
    pub success: Color,
    /// Error text
    pub error: Color,
    /// Warning/pending indicators
    pub warning: Color,
    /// Muted/secondary text
    pub muted: Color,
    /// Accent color (headers, highlights)
    pub accent: Color,
    /// Code block text
    pub code: Color,
    /// Border color
    pub border: Color,
    /// Dialog overlay background
    pub dialog_bg: Color,
    /// Selected item background
    pub selected_bg: Color,
    /// Selected item foreground
    pub selected_fg: Color,
}

impl Theme {
    /// Signet Dark — the default theme. Industrial monochrome with cyan accents.
    pub fn signet_dark() -> Self {
        Self {
            name: "signet-dark",
            status_bg: Color::Rgb(30, 30, 30),
            status_fg: Color::White,
            bg: Color::Reset,
            fg: Color::White,
            user: Color::Cyan,
            assistant: Color::Green,
            tool: Color::Magenta,
            success: Color::Green,
            error: Color::Red,
            warning: Color::Yellow,
            muted: Color::DarkGray,
            accent: Color::Cyan,
            code: Color::Green,
            border: Color::DarkGray,
            dialog_bg: Color::Rgb(20, 20, 20),
            selected_bg: Color::Cyan,
            selected_fg: Color::Black,
        }
    }

    /// Signet Light — for light terminal backgrounds.
    pub fn signet_light() -> Self {
        Self {
            name: "signet-light",
            status_bg: Color::Rgb(230, 230, 230),
            status_fg: Color::Black,
            bg: Color::Reset,
            fg: Color::Black,
            user: Color::Blue,
            assistant: Color::Rgb(0, 120, 0),
            tool: Color::Rgb(140, 0, 140),
            success: Color::Rgb(0, 120, 0),
            error: Color::Red,
            warning: Color::Rgb(180, 120, 0),
            muted: Color::Gray,
            accent: Color::Blue,
            code: Color::Rgb(0, 120, 0),
            border: Color::Gray,
            dialog_bg: Color::Rgb(240, 240, 240),
            selected_bg: Color::Blue,
            selected_fg: Color::White,
        }
    }

    /// Midnight — deep blue-black.
    pub fn midnight() -> Self {
        Self {
            name: "midnight",
            status_bg: Color::Rgb(15, 20, 35),
            status_fg: Color::Rgb(180, 190, 220),
            bg: Color::Reset,
            fg: Color::Rgb(200, 210, 230),
            user: Color::Rgb(100, 150, 255),
            assistant: Color::Rgb(80, 200, 120),
            tool: Color::Rgb(180, 130, 255),
            success: Color::Rgb(80, 200, 120),
            error: Color::Rgb(255, 100, 100),
            warning: Color::Rgb(255, 200, 80),
            muted: Color::Rgb(80, 90, 110),
            accent: Color::Rgb(100, 150, 255),
            code: Color::Rgb(80, 200, 120),
            border: Color::Rgb(50, 60, 80),
            dialog_bg: Color::Rgb(20, 25, 45),
            selected_bg: Color::Rgb(100, 150, 255),
            selected_fg: Color::Black,
        }
    }

    /// Amber — warm retro terminal aesthetic.
    pub fn amber() -> Self {
        Self {
            name: "amber",
            status_bg: Color::Rgb(30, 25, 15),
            status_fg: Color::Rgb(255, 180, 50),
            bg: Color::Reset,
            fg: Color::Rgb(255, 200, 100),
            user: Color::Rgb(255, 180, 50),
            assistant: Color::Rgb(255, 220, 130),
            tool: Color::Rgb(200, 150, 50),
            success: Color::Rgb(200, 255, 100),
            error: Color::Rgb(255, 80, 50),
            warning: Color::Rgb(255, 200, 50),
            muted: Color::Rgb(120, 100, 60),
            accent: Color::Rgb(255, 180, 50),
            code: Color::Rgb(255, 220, 130),
            border: Color::Rgb(80, 65, 30),
            dialog_bg: Color::Rgb(25, 20, 10),
            selected_bg: Color::Rgb(255, 180, 50),
            selected_fg: Color::Black,
        }
    }

    pub fn by_name(name: &str) -> Self {
        match name {
            "signet-light" | "light" => Self::signet_light(),
            "midnight" => Self::midnight(),
            "amber" => Self::amber(),
            _ => Self::signet_dark(),
        }
    }

    pub fn all_names() -> &'static [&'static str] {
        &["signet-dark", "signet-light", "midnight", "amber"]
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::signet_dark()
    }
}
