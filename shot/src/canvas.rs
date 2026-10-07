//! What is drawn on a picture: strokes of the highlighter and of the pen.

use gtk::cairo;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tool {
    Marker,
    Pen,
}

/// The inks: the name, the colour the highlighter lays down, the pen's colour.
pub const INKS: [(&str, [f64; 3], [f64; 3]); 4] = [
    ("Yellow", [1.0, 0.93, 0.25], [0.96, 0.76, 0.07]),
    ("Green", [0.56, 0.94, 0.62], [0.15, 0.64, 0.41]),
    ("Red", [1.0, 0.63, 0.72], [0.88, 0.11, 0.14]),
    ("Blue", [0.58, 0.83, 1.0], [0.11, 0.44, 0.85]),
];
/// Widths in the picture's pixels: the highlighter covers a line of text.
const MARKER_WIDTH: f64 = 18.0;
const PEN_WIDTH: f64 = 3.5;
/// How much of the pen's colour the highlighter adds to a dark page.
const GLOW: f64 = 0.36;

#[derive(Clone, Debug)]
pub struct Stroke {
    pub tool: Tool,
    pub ink: usize,
    pub points: Vec<(f64, f64)>,
}

impl Stroke {
    pub fn new(tool: Tool, ink: usize, at: (f64, f64)) -> Stroke {
        Stroke { tool, ink, points: vec![at] }
    }

    /// Draw the stroke. On a picture the highlighter darkens what is under
    /// it, as on paper, so dark text on a light page stays as sharp as it
    /// was; then it lightens it a little, which is what shows on a dark page,
    /// where there is nothing to darken. Over the live screen (while
    /// recording) there is no picture under it, and it is laid on half
    /// transparent.
    pub fn draw(&self, cr: &cairo::Context, on_picture: bool) {
        if self.points.is_empty() {
            return;
        }
        let (_, marker, pen) = INKS[self.ink % INKS.len()];
        let _ = cr.save();
        cr.set_line_cap(cairo::LineCap::Round);
        cr.set_line_join(cairo::LineJoin::Round);
        match self.tool {
            Tool::Marker if on_picture => {
                cr.set_operator(cairo::Operator::Multiply);
                cr.set_source_rgb(marker[0], marker[1], marker[2]);
                cr.set_line_width(MARKER_WIDTH);
                self.trace(cr);
                let _ = cr.stroke();
                cr.set_operator(cairo::Operator::Screen);
                cr.set_source_rgb(pen[0] * GLOW, pen[1] * GLOW, pen[2] * GLOW);
            }
            Tool::Marker => {
                cr.set_source_rgba(marker[0], marker[1] * 0.92, marker[2] * 0.2, 0.5);
                cr.set_line_width(MARKER_WIDTH);
            }
            Tool::Pen => {
                cr.set_source_rgb(pen[0], pen[1], pen[2]);
                cr.set_line_width(PEN_WIDTH);
            }
        }
        self.trace(cr);
        let _ = cr.stroke();
        let _ = cr.restore();
    }

    /// Is the stroke under this point (give or take `slack`)?
    pub fn covers(&self, at: (f64, f64), slack: f64) -> bool {
        let reach = slack + if self.tool == Tool::Marker { MARKER_WIDTH } else { PEN_WIDTH } / 2.0;
        let near = |a: (f64, f64), b: (f64, f64)| {
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let length = dx * dx + dy * dy;
            let along = if length > 0.0 { (((at.0 - a.0) * dx + (at.1 - a.1) * dy) / length).clamp(0.0, 1.0) } else { 0.0 };
            (at.0 - a.0 - along * dx).hypot(at.1 - a.1 - along * dy) <= reach
        };
        match self.points.as_slice() {
            [only] => near(*only, *only),
            points => points.windows(2).any(|pair| near(pair[0], pair[1])),
        }
    }

    /// The stroke as one path, so that where it crosses itself it is not
    /// darker.
    fn trace(&self, cr: &cairo::Context) {
        let Some(&(x, y)) = self.points.first() else { return };
        cr.move_to(x, y);
        for &(x, y) in &self.points[1..] {
            cr.line_to(x, y);
        }
        if self.points.len() == 1 {
            cr.line_to(x, y); // a dot
        }
    }
}

/// Where a straight stroke from `from` ends when the pointer is at `to`: level
/// or upright if it is nearly so (a line of text is highlighted that way).
pub fn straight(from: (f64, f64), to: (f64, f64)) -> (f64, f64) {
    let (dx, dy) = ((to.0 - from.0).abs(), (to.1 - from.1).abs());
    if dy <= dx * 0.27 {
        (to.0, from.1)
    } else if dx <= dy * 0.27 {
        (from.0, to.1)
    } else {
        to
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearly_level_becomes_level() {
        assert_eq!(straight((10.0, 10.0), (110.0, 22.0)), (110.0, 10.0));
        assert_eq!(straight((10.0, 10.0), (14.0, -90.0)), (10.0, -90.0));
        assert_eq!(straight((10.0, 10.0), (60.0, 60.0)), (60.0, 60.0));
    }

    #[test]
    fn stroke_under_a_point() {
        let mut pen = Stroke::new(Tool::Pen, 0, (10.0, 10.0));
        pen.points.push((100.0, 10.0));
        assert!(pen.covers((50.0, 12.0), 2.0));
        assert!(!pen.covers((50.0, 20.0), 2.0));
        assert!(!pen.covers((110.0, 10.0), 2.0));
        let dot = Stroke::new(Tool::Marker, 0, (30.0, 30.0));
        assert!(dot.covers((36.0, 30.0), 0.0));
        assert!(!dot.covers((45.0, 30.0), 0.0));
    }

    #[test]
    fn strokes_change_the_picture() {
        let picture = cairo::ImageSurface::create(cairo::Format::Rgb24, 40, 40).unwrap();
        {
            let cr = cairo::Context::new(&picture).unwrap();
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.paint().unwrap();
            let mut stroke = Stroke::new(Tool::Marker, 0, (5.0, 20.0));
            stroke.points.push((35.0, 20.0));
            stroke.draw(&cr, true);
            Stroke::new(Tool::Pen, 2, (20.0, 2.0)).draw(&cr, true);
        }
        let mut picture = picture;
        let stride = picture.stride() as usize;
        let data = picture.data().unwrap();
        let pixel = |x: usize, y: usize| (data[y * stride + x * 4 + 2], data[y * stride + x * 4 + 1], data[y * stride + x * 4]);
        // yellow highlighter on white: red and green stay, blue goes
        let (r, g, b) = pixel(20, 20);
        assert!(r > 250 && g > 230 && b < 80, "{r} {g} {b}");
        // the pen's red dot
        let (r, g, b) = pixel(20, 2);
        assert!(r > 200 && g < 60 && b < 60, "{r} {g} {b}");
        // untouched
        assert_eq!(pixel(2, 38), (255, 255, 255));
    }

    #[test]
    fn highlighter_shows_on_a_dark_page() {
        let mut picture = cairo::ImageSurface::create(cairo::Format::Rgb24, 40, 40).unwrap();
        {
            let cr = cairo::Context::new(&picture).unwrap();
            cr.set_source_rgb(0.1, 0.1, 0.1);
            cr.paint().unwrap();
            let mut stroke = Stroke::new(Tool::Marker, 0, (5.0, 20.0));
            stroke.points.push((35.0, 20.0));
            stroke.draw(&cr, true);
        }
        let stride = picture.stride() as usize;
        let data = picture.data().unwrap();
        let (r, g, b) = (data[20 * stride + 82], data[20 * stride + 81], data[20 * stride + 80]);
        assert!(r > 80 && g > 60 && b < 40, "{r} {g} {b}");
    }
}
