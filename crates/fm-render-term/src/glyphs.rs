//! Unicode box-drawing and ASCII fallback glyphs.

use fm_core::MermaidGlyphMode;

/// Box-drawing character set.
#[derive(Debug, Clone, Copy)]
pub struct BoxGlyphs {
    pub horizontal: char,
    pub vertical: char,
    pub top_left: char,
    pub top_right: char,
    pub bottom_left: char,
    pub bottom_right: char,
    pub cross: char,
    pub t_down: char,
    pub t_up: char,
    pub t_left: char,
    pub t_right: char,
    pub arrow_right: char,
    pub arrow_left: char,
    pub arrow_up: char,
    pub arrow_down: char,
    pub diamond: char,
    pub circle: char,
    pub block_full: char,
    pub block_light: char,
    pub block_medium: char,
}

impl BoxGlyphs {
    /// Unicode box-drawing characters (light).
    pub const UNICODE: Self = Self {
        horizontal: '─',
        vertical: '│',
        top_left: '┌',
        top_right: '┐',
        bottom_left: '└',
        bottom_right: '┘',
        cross: '┼',
        t_down: '┬',
        t_up: '┴',
        t_left: '┤',
        t_right: '├',
        arrow_right: '▶',
        arrow_left: '◀',
        arrow_up: '▲',
        arrow_down: '▼',
        diamond: '◇',
        circle: '○',
        block_full: '█',
        block_light: '░',
        block_medium: '▒',
    };

    /// Unicode heavy box-drawing characters.
    pub const UNICODE_HEAVY: Self = Self {
        horizontal: '━',
        vertical: '┃',
        top_left: '┏',
        top_right: '┓',
        bottom_left: '┗',
        bottom_right: '┛',
        cross: '╋',
        t_down: '┳',
        t_up: '┻',
        t_left: '┫',
        t_right: '┣',
        arrow_right: '▶',
        arrow_left: '◀',
        arrow_up: '▲',
        arrow_down: '▼',
        diamond: '◆',
        circle: '●',
        block_full: '█',
        block_light: '░',
        block_medium: '▒',
    };

    /// Unicode double-line box-drawing characters.
    pub const UNICODE_DOUBLE: Self = Self {
        horizontal: '═',
        vertical: '║',
        top_left: '╔',
        top_right: '╗',
        bottom_left: '╚',
        bottom_right: '╝',
        cross: '╬',
        t_down: '╦',
        t_up: '╩',
        t_left: '╣',
        t_right: '╠',
        arrow_right: '»',
        arrow_left: '«',
        arrow_up: '▲',
        arrow_down: '▼',
        diamond: '◇',
        circle: '○',
        block_full: '█',
        block_light: '░',
        block_medium: '▒',
    };

    /// ASCII fallback characters.
    pub const ASCII: Self = Self {
        horizontal: '-',
        vertical: '|',
        top_left: '+',
        top_right: '+',
        bottom_left: '+',
        bottom_right: '+',
        cross: '+',
        t_down: '+',
        t_up: '+',
        t_left: '+',
        t_right: '+',
        arrow_right: '>',
        arrow_left: '<',
        arrow_up: '^',
        arrow_down: 'v',
        diamond: '*',
        circle: 'o',
        block_full: '#',
        block_light: '.',
        block_medium: ':',
    };

    /// Unicode rounded box-drawing characters.
    pub const UNICODE_ROUNDED: Self = Self {
        horizontal: '─',
        vertical: '│',
        top_left: '╭',
        top_right: '╮',
        bottom_left: '╰',
        bottom_right: '╯',
        cross: '┼',
        t_down: '┬',
        t_up: '┴',
        t_left: '┤',
        t_right: '├',
        arrow_right: '▶',
        arrow_left: '◀',
        arrow_up: '▲',
        arrow_down: '▼',
        diamond: '◇',
        circle: '○',
        block_full: '█',
        block_light: '░',
        block_medium: '▒',
    };

    /// Get the appropriate glyph set for the mode.
    #[must_use]
    pub const fn for_mode(mode: MermaidGlyphMode) -> Self {
        match mode {
            MermaidGlyphMode::Unicode => Self::UNICODE,
            MermaidGlyphMode::Ascii => Self::ASCII,
        }
    }
}

/// Node shape characters for compact rendering.
#[derive(Debug, Clone, Copy)]
pub struct ShapeGlyphs {
    pub rect_tl: char,
    pub rect_tr: char,
    pub rect_bl: char,
    pub rect_br: char,
    pub rounded_tl: char,
    pub rounded_tr: char,
    pub rounded_bl: char,
    pub rounded_br: char,
    pub diamond_top: char,
    pub diamond_left: char,
    pub diamond_right: char,
    pub diamond_bottom: char,
    pub circle: char,
    pub hexagon_left: char,
    pub hexagon_right: char,
    pub parallelogram_tl: char,
    pub parallelogram_br: char,
}

impl ShapeGlyphs {
    /// Unicode shape characters.
    pub const UNICODE: Self = Self {
        rect_tl: '┌',
        rect_tr: '┐',
        rect_bl: '└',
        rect_br: '┘',
        rounded_tl: '╭',
        rounded_tr: '╮',
        rounded_bl: '╰',
        rounded_br: '╯',
        diamond_top: '◇',
        diamond_left: '◇',
        diamond_right: '◇',
        diamond_bottom: '◇',
        circle: '○',
        hexagon_left: '⬡',
        hexagon_right: '⬡',
        parallelogram_tl: '/',
        parallelogram_br: '/',
    };

    /// ASCII shape characters.
    pub const ASCII: Self = Self {
        rect_tl: '+',
        rect_tr: '+',
        rect_bl: '+',
        rect_br: '+',
        rounded_tl: '(',
        rounded_tr: ')',
        rounded_bl: '(',
        rounded_br: ')',
        diamond_top: '/',
        diamond_left: '<',
        diamond_right: '>',
        diamond_bottom: '\\',
        circle: 'O',
        hexagon_left: '<',
        hexagon_right: '>',
        parallelogram_tl: '/',
        parallelogram_br: '/',
    };

    /// Get the appropriate shape glyphs for the mode.
    #[must_use]
    pub const fn for_mode(mode: MermaidGlyphMode) -> Self {
        match mode {
            MermaidGlyphMode::Unicode => Self::UNICODE,
            MermaidGlyphMode::Ascii => Self::ASCII,
        }
    }
}

/// Edge/Arrow characters.
#[derive(Debug, Clone, Copy)]
pub struct EdgeGlyphs {
    pub line_h: char,
    pub line_v: char,
    pub line_diag_ne: char,
    pub line_diag_nw: char,
    pub arrow_right: char,
    pub arrow_left: char,
    pub arrow_up: char,
    pub arrow_down: char,
    pub arrow_thick_right: char,
    pub arrow_thick_left: char,
    pub dotted_h: char,
    pub dotted_v: char,
    pub circle_head: char,
    pub cross_head: char,
}

impl EdgeGlyphs {
    /// Unicode edge characters.
    pub const UNICODE: Self = Self {
        line_h: '─',
        line_v: '│',
        line_diag_ne: '╱',
        line_diag_nw: '╲',
        arrow_right: '▶',
        arrow_left: '◀',
        arrow_up: '▲',
        arrow_down: '▼',
        arrow_thick_right: '▸',
        arrow_thick_left: '◂',
        dotted_h: '┄',
        dotted_v: '┆',
        circle_head: '●',
        cross_head: '✕',
    };

    /// ASCII edge characters.
    pub const ASCII: Self = Self {
        line_h: '-',
        line_v: '|',
        line_diag_ne: '/',
        line_diag_nw: '\\',
        arrow_right: '>',
        arrow_left: '<',
        arrow_up: '^',
        arrow_down: 'v',
        arrow_thick_right: '>',
        arrow_thick_left: '<',
        dotted_h: '.',
        dotted_v: ':',
        circle_head: 'o',
        cross_head: 'x',
    };

    /// Get the appropriate edge glyphs for the mode.
    #[must_use]
    pub const fn for_mode(mode: MermaidGlyphMode) -> Self {
        match mode {
            MermaidGlyphMode::Unicode => Self::UNICODE,
            MermaidGlyphMode::Ascii => Self::ASCII,
        }
    }
}

/// Cluster/subgraph decoration characters.
#[derive(Debug, Clone, Copy)]
pub struct ClusterGlyphs {
    pub border_h: char,
    pub border_v: char,
    pub corner_tl: char,
    pub corner_tr: char,
    pub corner_bl: char,
    pub corner_br: char,
    pub title_left: char,
    pub title_right: char,
    pub fill: char,
}

impl ClusterGlyphs {
    /// Unicode cluster characters.
    pub const UNICODE: Self = Self {
        border_h: '┈',
        border_v: '┊',
        corner_tl: '┌',
        corner_tr: '┐',
        corner_bl: '└',
        corner_br: '┘',
        title_left: '┤',
        title_right: '├',
        fill: '░',
    };

    /// ASCII cluster characters.
    pub const ASCII: Self = Self {
        border_h: '-',
        border_v: ':',
        corner_tl: '+',
        corner_tr: '+',
        corner_bl: '+',
        corner_br: '+',
        title_left: '[',
        title_right: ']',
        fill: '.',
    };

    /// Get the appropriate cluster glyphs for the mode.
    #[must_use]
    pub const fn for_mode(mode: MermaidGlyphMode) -> Self {
        match mode {
            MermaidGlyphMode::Unicode => Self::UNICODE,
            MermaidGlyphMode::Ascii => Self::ASCII,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_glyphs_are_distinct() {
        let glyphs = BoxGlyphs::UNICODE;
        assert_ne!(glyphs.horizontal, glyphs.vertical);
        assert_ne!(glyphs.top_left, glyphs.bottom_right);
    }

    #[test]
    fn ascii_fallback_is_printable() {
        let glyphs = BoxGlyphs::ASCII;
        assert!(glyphs.horizontal.is_ascii());
        assert!(glyphs.vertical.is_ascii());
        assert!(glyphs.arrow_right.is_ascii());
    }

    #[test]
    fn mode_selects_correct_glyphs() {
        let unicode = BoxGlyphs::for_mode(MermaidGlyphMode::Unicode);
        let ascii = BoxGlyphs::for_mode(MermaidGlyphMode::Ascii);
        assert_eq!(unicode.horizontal, '─');
        assert_eq!(ascii.horizontal, '-');
    }
}
