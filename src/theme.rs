use ratatui::style::Color;

#[derive(Debug, Clone)]
pub struct Theme {
    pub border_inactive: Color,
    pub border_active: Color,
    pub primary: Color,   // Cyan
    pub secondary: Color, // Violet / Purple
    pub success: Color,   // Emerald / Green
    pub warning: Color,   // Amber / Yellow
    pub error: Color,     // Rose / Red
    pub text: Color,
    pub text_muted: Color,
    pub text_dim: Color,
    pub code_bg: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            border_inactive: Color::Rgb(55, 65, 81), // Slate Gray
            border_active: Color::Rgb(56, 189, 248), // Sky Cyan
            primary: Color::Rgb(56, 189, 248),       // Sky Cyan
            secondary: Color::Rgb(168, 85, 247),     // Purple/Violet
            success: Color::Rgb(52, 211, 153),       // Emerald Green
            warning: Color::Rgb(251, 191, 36),       // Amber Yellow
            error: Color::Rgb(248, 113, 113),        // Rose Red
            text: Color::Rgb(243, 244, 246),         // Soft White
            text_muted: Color::Rgb(156, 163, 175),   // Gray
            text_dim: Color::Rgb(107, 114, 128),     // Darker Gray
            code_bg: Color::Rgb(30, 41, 59),         // Slate Code Background
        }
    }
}
