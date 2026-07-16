//! Sub-cell pixel canvas for terminal rendering.
//!
//! Supports three sub-cell modes:
//! - Braille (2x4): Highest resolution using Unicode Braille characters (U+2800-U+28FF)
//! - Block (2x2): Quarter block characters (U+2596-U+259F)
//! - HalfBlock (1x2): Half block characters (▀▄█ )

use fm_core::MermaidRenderMode;

/// A pixel-level canvas that maps to terminal cells.
#[derive(Debug, Clone)]
pub struct Canvas {
    /// Fused pixel + generation buffer: `cell_gen[i] == generation` iff the pixel is set THIS
    /// generation (0 = unset / never touched). This unifies the old `pixels: Vec<bool>` +
    /// `pixel_gen: Vec<u32>` — a set/get touched BOTH (two separate allocations / cache lines) even
    /// though `pixels[i]` was only meaningful when `pixel_gen[i] == generation`. Now one read/write per
    /// pixel op. Byte-identical: set writes `generation`, unset writes 0, get is `== generation`
    /// (`generation` is never 0 — starts at 1, `clear` wraps to 1 — so 0 always reads as unset).
    cell_gen: Vec<u32>,
    /// Width in pixels.
    pixel_width: usize,
    /// Height in pixels.
    pixel_height: usize,
    /// Width in terminal cells.
    cell_width: usize,
    /// Height in terminal cells.
    cell_height: usize,
    /// Render mode determining sub-cell resolution.
    mode: MermaidRenderMode,
    /// Generation counter for O(1) clear.
    generation: u32,
}

impl Canvas {
    /// Create a new canvas with the given cell dimensions.
    #[must_use]
    pub fn new(cell_width: usize, cell_height: usize, mode: MermaidRenderMode) -> Self {
        let (mult_x, mult_y) = subcell_multiplier(mode);
        let pixel_width = cell_width.saturating_mul(mult_x);
        let pixel_height = cell_height.saturating_mul(mult_y);
        let size = pixel_width.saturating_mul(pixel_height);

        Self {
            cell_gen: vec![0; size],
            pixel_width,
            pixel_height,
            cell_width,
            cell_height,
            mode,
            generation: 1,
        }
    }

    /// Clear the canvas (O(1) using generation counter).
    pub fn clear(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            // Wrapped around, need to reset everything.
            self.generation = 1;
            self.cell_gen.fill(0);
        }
    }

    /// Get the pixel dimensions.
    #[must_use]
    pub const fn pixel_dimensions(&self) -> (usize, usize) {
        (self.pixel_width, self.pixel_height)
    }

    /// Get the cell dimensions.
    #[must_use]
    pub const fn cell_dimensions(&self) -> (usize, usize) {
        (self.cell_width, self.cell_height)
    }

    /// Set a pixel at (x, y).
    pub fn set_pixel(&mut self, x: usize, y: usize) {
        if let Some(index) = self.pixel_index(x, y) {
            self.cell_gen[index] = self.generation;
        }
    }

    /// Unset a pixel at (x, y).
    pub fn unset_pixel(&mut self, x: usize, y: usize) {
        if let Some(index) = self.pixel_index(x, y) {
            // 0 reads as unset for any `generation >= 1`; matches the old `pixels[i]=false` result
            // (the old `pixel_gen[i]=generation` write was unobservable — `get` returned false anyway).
            self.cell_gen[index] = 0;
        }
    }

    /// Get the value of a pixel at (x, y).
    #[must_use]
    pub fn get_pixel(&self, x: usize, y: usize) -> bool {
        self.pixel_index(x, y)
            .map(|index| self.pixel_set_at(index))
            .unwrap_or(false)
    }

    /// Whether the pixel at a KNOWN-valid buffer `index` is set this generation. The caller must have
    /// established `index < cell_gen.len()` (e.g. from a cell wholly inside the pixel grid); this is the
    /// body of [`get_pixel`] without the per-call `pixel_index` bounds-check + multiply + `Option`, so a
    /// cell renderer that reads several fixed-offset sub-pixels off one precomputed base index skips that
    /// recompute per sub-pixel. Byte-identical to `get_pixel(x, y)` for the in-bounds `(x, y)` that maps
    /// to `index`.
    #[inline]
    fn pixel_set_at(&self, index: usize) -> bool {
        self.cell_gen[index] == self.generation
    }

    /// Draw a line from (x0, y0) to (x1, y1) using Bresenham's algorithm.
    pub fn draw_line(&mut self, x0: isize, y0: isize, x1: isize, y1: isize) {
        // Axis-aligned fast paths. Rect outlines (`draw_rect`) and orthogonal connector segments are
        // horizontal or vertical, and Bresenham's per-pixel `set_pixel` recomputes `pixel_index`
        // (bounds check + multiply + `Option`) and stamps two buffers on every step. A horizontal run has
        // CONTIGUOUS indices, so one `fill(true)`/`fill(generation)` replaces the whole loop; a vertical
        // run has a fixed `pixel_width` stride, so the index increments with no per-pixel multiply.
        // Byte-identical to the Bresenham path: it sets exactly the same in-bounds pixel set (0≤x<width,
        // 0≤y<height, endpoints inclusive — pixel order is irrelevant to the final buffer) with the same
        // `true` + `generation` stamp.
        if y0 == y1 {
            if y0 >= 0 && (y0 as usize) < self.pixel_height {
                let x_lo = x0.min(x1).max(0);
                let x_hi = x0.max(x1).min(self.pixel_width as isize - 1);
                if x_lo <= x_hi {
                    let base = (y0 as usize) * self.pixel_width;
                    let lo = base + x_lo as usize;
                    let hi = base + x_hi as usize;
                    self.cell_gen[lo..=hi].fill(self.generation);
                }
            }
            return;
        }
        if x0 == x1 {
            if x0 >= 0 && (x0 as usize) < self.pixel_width {
                let y_lo = y0.min(y1).max(0);
                let y_hi = y0.max(y1).min(self.pixel_height as isize - 1);
                if y_lo <= y_hi {
                    let x = x0 as usize;
                    let mut index = (y_lo as usize) * self.pixel_width + x;
                    for _ in y_lo..=y_hi {
                        self.cell_gen[index] = self.generation;
                        index += self.pixel_width;
                    }
                }
            }
            return;
        }

        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let mut x = x0;
        let mut y = y0;

        loop {
            if x >= 0 && y >= 0 {
                self.set_pixel(x as usize, y as usize);
            }

            if x == x1 && y == y1 {
                break;
            }

            let e2 = 2 * err;
            if e2 >= dy {
                if x == x1 {
                    break;
                }
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                if y == y1 {
                    break;
                }
                err += dx;
                y += sy;
            }
        }
    }

    /// Draw a rectangle outline.
    pub fn draw_rect(&mut self, x: usize, y: usize, width: usize, height: usize) {
        if width == 0 || height == 0 {
            return;
        }

        let x0 = x as isize;
        let y0 = y as isize;
        let x1 = (x + width - 1) as isize;
        let y1 = (y + height - 1) as isize;

        self.draw_line(x0, y0, x1, y0); // Top
        self.draw_line(x0, y1, x1, y1); // Bottom
        self.draw_line(x0, y0, x0, y1); // Left
        self.draw_line(x1, y0, x1, y1); // Right
    }

    /// Fill a rectangle.
    pub fn fill_rect(&mut self, x: usize, y: usize, width: usize, height: usize) {
        for dy in 0..height {
            for dx in 0..width {
                self.set_pixel(x + dx, y + dy);
            }
        }
    }

    /// Draw a circle outline using midpoint algorithm.
    pub fn draw_circle(&mut self, cx: isize, cy: isize, radius: isize) {
        if radius <= 0 {
            if radius == 0 && cx >= 0 && cy >= 0 {
                self.set_pixel(cx as usize, cy as usize);
            }
            return;
        }

        let mut x = radius;
        let mut y = 0_isize;
        let mut err = 1 - radius;

        while x >= y {
            self.set_circle_octants(cx, cy, x, y);
            y += 1;
            if err < 0 {
                err += 2 * y + 1;
            } else {
                x -= 1;
                err += 2 * (y - x) + 1;
            }
        }
    }

    fn set_circle_octants(&mut self, cx: isize, cy: isize, x: isize, y: isize) {
        let points = [
            (cx + x, cy + y),
            (cx - x, cy + y),
            (cx + x, cy - y),
            (cx - x, cy - y),
            (cx + y, cy + x),
            (cx - y, cy + x),
            (cx + y, cy - x),
            (cx - y, cy - x),
        ];
        for (px, py) in points {
            if px >= 0 && py >= 0 {
                self.set_pixel(px as usize, py as usize);
            }
        }
    }

    /// Fill a circle.
    pub fn fill_circle(&mut self, cx: isize, cy: isize, radius: isize) {
        if radius <= 0 {
            if radius == 0 && cx >= 0 && cy >= 0 {
                self.set_pixel(cx as usize, cy as usize);
            }
            return;
        }

        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy <= radius * radius {
                    let px = cx + dx;
                    let py = cy + dy;
                    if px >= 0 && py >= 0 {
                        self.set_pixel(px as usize, py as usize);
                    }
                }
            }
        }
    }

    /// Render the canvas to a string of terminal characters.
    #[must_use]
    pub fn render(&self) -> String {
        let mut output = String::with_capacity(self.cell_width * self.cell_height * 4);

        for cell_y in 0..self.cell_height {
            if cell_y > 0 {
                output.push('\n');
            }
            for cell_x in 0..self.cell_width {
                let ch = self.render_cell(cell_x, cell_y);
                output.push(ch);
            }
        }

        output
    }

    /// Render a single cell to its character.
    #[must_use]
    fn render_cell(&self, cell_x: usize, cell_y: usize) -> char {
        match self.mode {
            MermaidRenderMode::Braille => self.render_braille_cell(cell_x, cell_y),
            MermaidRenderMode::Block => self.render_block_cell(cell_x, cell_y),
            MermaidRenderMode::HalfBlock => self.render_halfblock_cell(cell_x, cell_y),
            MermaidRenderMode::CellOnly | MermaidRenderMode::Auto => {
                if self.get_pixel(cell_x, cell_y) {
                    '█'
                } else {
                    ' '
                }
            }
        }
    }

    /// Render a 2x4 pixel block as a Braille character.
    fn render_braille_cell(&self, cell_x: usize, cell_y: usize) -> char {
        // Braille dot pattern:
        // 0 3
        // 1 4
        // 2 5
        // 6 7
        let px = cell_x * 2;
        let py = cell_y * 4;

        let mut code_point = 0x2800_u32; // Unicode Braille base

        // Fast path: the whole 2x4 sub-pixel block is inside the grid, so compute the base index ONCE
        // and read the 8 dots by fixed offset (`base + row*width + col`) instead of 8 `get_pixel` calls,
        // each of which recomputes `pixel_index` (bounds check + `y*width+x` multiply + `Option`). This
        // is the default (Braille) render mode, called once per output cell. Byte-identical: every
        // `pixel_set_at(base + r*w + c)` equals `get_pixel(px+c, py+r)` because the gate guarantees each
        // `(px+c, py+r)` is in bounds and maps to exactly that index.
        if px + 1 < self.pixel_width && py + 3 < self.pixel_height {
            let w = self.pixel_width;
            let base = py * w + px;
            if self.pixel_set_at(base) {
                code_point |= 0x01;
            } // Dot 1 (px, py)
            if self.pixel_set_at(base + w) {
                code_point |= 0x02;
            } // Dot 2 (px, py+1)
            if self.pixel_set_at(base + 2 * w) {
                code_point |= 0x04;
            } // Dot 3 (px, py+2)
            if self.pixel_set_at(base + 1) {
                code_point |= 0x08;
            } // Dot 4 (px+1, py)
            if self.pixel_set_at(base + w + 1) {
                code_point |= 0x10;
            } // Dot 5 (px+1, py+1)
            if self.pixel_set_at(base + 2 * w + 1) {
                code_point |= 0x20;
            } // Dot 6 (px+1, py+2)
            if self.pixel_set_at(base + 3 * w) {
                code_point |= 0x40;
            } // Dot 7 (px, py+3)
            if self.pixel_set_at(base + 3 * w + 1) {
                code_point |= 0x80;
            } // Dot 8 (px+1, py+3)
        } else {
            // Edge cell: bounds-safe per-pixel reads (unchanged).
            if self.get_pixel(px, py) {
                code_point |= 0x01;
            }
            if self.get_pixel(px, py + 1) {
                code_point |= 0x02;
            }
            if self.get_pixel(px, py + 2) {
                code_point |= 0x04;
            }
            if self.get_pixel(px + 1, py) {
                code_point |= 0x08;
            }
            if self.get_pixel(px + 1, py + 1) {
                code_point |= 0x10;
            }
            if self.get_pixel(px + 1, py + 2) {
                code_point |= 0x20;
            }
            if self.get_pixel(px, py + 3) {
                code_point |= 0x40;
            }
            if self.get_pixel(px + 1, py + 3) {
                code_point |= 0x80;
            }
        }

        char::from_u32(code_point).unwrap_or(' ')
    }

    /// Render a 2x2 pixel block as a quarter block character.
    fn render_block_cell(&self, cell_x: usize, cell_y: usize) -> char {
        let px = cell_x * 2;
        let py = cell_y * 2;

        let tl = self.get_pixel(px, py);
        let tr = self.get_pixel(px + 1, py);
        let bl = self.get_pixel(px, py + 1);
        let br = self.get_pixel(px + 1, py + 1);

        match (tl, tr, bl, br) {
            (false, false, false, false) => ' ',
            (true, false, false, false) => '▘',
            (false, true, false, false) => '▝',
            (true, true, false, false) => '▀',
            (false, false, true, false) => '▖',
            (true, false, true, false) => '▌',
            (false, true, true, false) => '▞',
            (true, true, true, false) => '▛',
            (false, false, false, true) => '▗',
            (true, false, false, true) => '▚',
            (false, true, false, true) => '▐',
            (true, true, false, true) => '▜',
            (false, false, true, true) => '▄',
            (true, false, true, true) => '▙',
            (false, true, true, true) => '▟',
            (true, true, true, true) => '█',
        }
    }

    /// Render a 1x2 pixel block as a half block character.
    fn render_halfblock_cell(&self, cell_x: usize, cell_y: usize) -> char {
        let py = cell_y * 2;

        let top = self.get_pixel(cell_x, py);
        let bottom = self.get_pixel(cell_x, py + 1);

        match (top, bottom) {
            (false, false) => ' ',
            (true, false) => '▀',
            (false, true) => '▄',
            (true, true) => '█',
        }
    }

    fn pixel_index(&self, x: usize, y: usize) -> Option<usize> {
        if x < self.pixel_width && y < self.pixel_height {
            Some(y * self.pixel_width + x)
        } else {
            None
        }
    }
}

/// Get the sub-cell multiplier for a render mode.
#[must_use]
pub const fn subcell_multiplier(mode: MermaidRenderMode) -> (usize, usize) {
    match mode {
        MermaidRenderMode::Braille => (2, 4),
        MermaidRenderMode::Block => (2, 2),
        MermaidRenderMode::HalfBlock => (1, 2),
        MermaidRenderMode::CellOnly | MermaidRenderMode::Auto => (1, 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_canvas_starts_empty() {
        let canvas = Canvas::new(10, 5, MermaidRenderMode::Braille);
        assert_eq!(canvas.pixel_dimensions(), (20, 20));
        assert_eq!(canvas.cell_dimensions(), (10, 5));
        assert!(!canvas.get_pixel(0, 0));
    }

    #[test]
    fn set_and_get_pixel() {
        let mut canvas = Canvas::new(10, 5, MermaidRenderMode::Braille);
        canvas.set_pixel(5, 10);
        assert!(canvas.get_pixel(5, 10));
        assert!(!canvas.get_pixel(5, 11));
    }

    #[test]
    fn clear_resets_pixels() {
        let mut canvas = Canvas::new(10, 5, MermaidRenderMode::Braille);
        canvas.set_pixel(5, 10);
        canvas.clear();
        assert!(!canvas.get_pixel(5, 10));
    }

    #[test]
    fn draw_line_horizontal() {
        let mut canvas = Canvas::new(10, 5, MermaidRenderMode::CellOnly);
        canvas.draw_line(0, 0, 5, 0);
        for x in 0..=5 {
            assert!(canvas.get_pixel(x, 0), "pixel ({x}, 0) should be set");
        }
        assert!(!canvas.get_pixel(6, 0));
    }

    #[test]
    fn draw_line_diagonal() {
        let mut canvas = Canvas::new(10, 10, MermaidRenderMode::CellOnly);
        canvas.draw_line(0, 0, 5, 5);
        // Bresenham should hit all diagonal pixels
        for i in 0..=5 {
            assert!(canvas.get_pixel(i, i), "pixel ({i}, {i}) should be set");
        }
    }

    #[test]
    fn braille_renders_correctly() {
        let mut canvas = Canvas::new(1, 1, MermaidRenderMode::Braille);
        // Set all 8 dots
        canvas.set_pixel(0, 0);
        canvas.set_pixel(0, 1);
        canvas.set_pixel(0, 2);
        canvas.set_pixel(0, 3);
        canvas.set_pixel(1, 0);
        canvas.set_pixel(1, 1);
        canvas.set_pixel(1, 2);
        canvas.set_pixel(1, 3);
        let output = canvas.render();
        assert_eq!(output, "⣿"); // Full braille character
    }

    #[test]
    fn halfblock_renders_correctly() {
        let mut canvas = Canvas::new(1, 1, MermaidRenderMode::HalfBlock);
        canvas.set_pixel(0, 0);
        assert_eq!(canvas.render(), "▀");

        canvas.clear();
        canvas.set_pixel(0, 1);
        assert_eq!(canvas.render(), "▄");

        canvas.set_pixel(0, 0);
        assert_eq!(canvas.render(), "█");
    }

    #[test]
    fn block_renders_all_patterns() {
        let mut canvas = Canvas::new(1, 1, MermaidRenderMode::Block);
        assert_eq!(canvas.render(), " ");

        canvas.set_pixel(0, 0);
        canvas.set_pixel(1, 0);
        assert_eq!(canvas.render(), "▀");

        canvas.set_pixel(0, 1);
        canvas.set_pixel(1, 1);
        assert_eq!(canvas.render(), "█");
    }
}
