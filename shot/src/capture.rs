//! The picture of the screen: taken with grim, cut to what was chosen, written
//! as a PNG file and put on the clipboard.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;
use std::{fs, thread};

use gtk::cairo::{self, Format, ImageSurface};
use gtk::glib;

const GRIM_LIMIT: Duration = Duration::from_secs(3);

/// A rectangle in screen units (what GTK measures in).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    /// The rectangle between two points that lies inside `0,0 .. w,h`, on
    /// whole units.
    pub fn between(a: (f64, f64), b: (f64, f64), w: f64, h: f64) -> Rect {
        let (x0, x1) = (a.0.min(b.0).clamp(0.0, w).round(), a.0.max(b.0).clamp(0.0, w).round());
        let (y0, y1) = (a.1.min(b.1).clamp(0.0, h).round(), a.1.max(b.1).clamp(0.0, h).round());
        Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
    }
}

/// A rectangle in the pixels of a picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PixelRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// The frozen screen: all monitors in one picture, as grim lays them out.
pub struct Shot {
    pub surface: ImageSurface,
    pub width: i32,
    pub height: i32,
}

impl Shot {
    pub fn all(&self) -> PixelRect {
        PixelRect { x: 0, y: 0, w: self.width, h: self.height }
    }

    /// The pixels of a rectangle given in screen units from the picture's
    /// corner, where one unit is `scale` pixels.
    pub fn part(&self, r: Rect, scale: f64) -> PixelRect {
        let x0 = ((r.x * scale).round() as i32).clamp(0, self.width - 1);
        let y0 = ((r.y * scale).round() as i32).clamp(0, self.height - 1);
        let x1 = (((r.x + r.w) * scale).round() as i32).clamp(x0 + 1, self.width);
        let y1 = (((r.y + r.h) * scale).round() as i32).clamp(y0 + 1, self.height);
        PixelRect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
    }

    /// A part of the picture as a picture of its own.
    pub fn cut(&self, r: PixelRect) -> Option<ImageSurface> {
        let part = ImageSurface::create(Format::Rgb24, r.w, r.h).ok()?;
        let cr = cairo::Context::new(&part).ok()?;
        cr.set_source_surface(&self.surface, -f64::from(r.x), -f64::from(r.y)).ok()?;
        cr.paint().ok()?;
        Some(part)
    }
}

impl Shot {
    /// The part of the picture inside a shape drawn by hand (its corner
    /// points, in pixels), as a picture of its own that is see-through
    /// outside the shape.
    pub fn cut_shape(&self, outline: &[(f64, f64)]) -> Option<ImageSurface> {
        let (mut left, mut top, mut right, mut bottom) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for &(x, y) in outline {
            (left, top, right, bottom) = (left.min(x), top.min(y), right.max(x), bottom.max(y));
        }
        if outline.len() < 3 {
            return None;
        }
        let around = self.part(Rect { x: left, y: top, w: right - left, h: bottom - top }, 1.0);
        let part = ImageSurface::create(Format::ARgb32, around.w, around.h).ok()?;
        let cr = cairo::Context::new(&part).ok()?;
        cr.translate(-f64::from(around.x), -f64::from(around.y));
        for (i, &(x, y)) in outline.iter().enumerate() {
            if i == 0 {
                cr.move_to(x, y);
            } else {
                cr.line_to(x, y);
            }
        }
        cr.close_path();
        cr.clip();
        cr.set_source_surface(&self.surface, 0.0, 0.0).ok()?;
        cr.paint().ok()?;
        Some(part)
    }
}

/// Start taking the picture. grim writes it as PPM, which costs it no time to
/// pack (as PNG the whole screen takes a few tenths of a second on the laptop).
pub fn grab() -> Option<Child> {
    Command::new("grim")
        .args(["-t", "ppm", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()
}

/// Wait for grim and read what it took. It is given up if it takes too long:
/// grim waits for ever for a screen that is switched off.
pub fn finish(mut grim: Child) -> Option<Shot> {
    let mut stdout = grim.stdout.take()?;
    let (send, receive) = mpsc::channel();
    thread::spawn(move || {
        let mut file = Vec::new();
        let read = stdout.read_to_end(&mut file).is_ok();
        let _ = send.send(read.then_some(file));
    });
    let file = receive.recv_timeout(GRIM_LIMIT).ok().flatten();
    if file.is_none() {
        let _ = grim.kill();
    }
    if !grim.wait().is_ok_and(|status| status.success()) {
        return None;
    }
    let file = file?;
    let (width, height, rgb) = parse_ppm(&file)?;
    // cairo wants four bytes a pixel: blue, green, red, one unused
    let mut data = vec![0u8; rgb.len() / 3 * 4];
    for (to, from) in data.as_chunks_mut::<4>().0.iter_mut().zip(rgb.as_chunks::<3>().0) {
        (to[0], to[1], to[2]) = (from[2], from[1], from[0]);
    }
    let surface = ImageSurface::create_for_data(data, Format::Rgb24, width, height, width * 4).ok()?;
    Some(Shot { surface, width, height })
}

/// A binary PPM ("P6") with one byte per colour: its size and its pixels.
fn parse_ppm(file: &[u8]) -> Option<(i32, i32, &[u8])> {
    let mut at = 0;
    let mut word = || {
        while file.get(at)?.is_ascii_whitespace() {
            at += 1;
        }
        let from = at;
        while !file.get(at)?.is_ascii_whitespace() {
            at += 1;
        }
        std::str::from_utf8(&file[from..at]).ok()
    };
    if word()? != "P6" {
        return None;
    }
    let width: i32 = word()?.parse().ok()?;
    let height: i32 = word()?.parse().ok()?;
    if word()? != "255" || width < 1 || height < 1 {
        return None;
    }
    // one white-space character, then the pixels
    let pixels = file.get(at + 1..)?;
    let size = usize::try_from(width).ok()?.checked_mul(usize::try_from(height).ok()?)?.checked_mul(3)?;
    (pixels.len() >= size).then(|| (width, height, &pixels[..size]))
}

pub fn png(picture: &ImageSurface) -> Option<Vec<u8>> {
    let mut file = Vec::new();
    picture.write_to_png(&mut file).ok()?;
    Some(file)
}

/// Put a PNG on the clipboard. wl-copy stays behind to hand it out, and
/// toros-clipd files it in the clipboard history (Super+V).
pub fn copy(png: &[u8]) -> bool {
    let wl_copy = Command::new("wl-copy")
        .args(["--type", "image/png"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut wl_copy) = wl_copy else { return false };
    let written = wl_copy.stdin.take().is_some_and(|mut stdin| stdin.write_all(png).is_ok());
    wl_copy.wait().is_ok_and(|status| status.success()) && written
}

/// A new file for what is taken now: "Screenshot 2026-10-06 21-30-05.png" in
/// Pictures/Screenshots, "Recording ….mp4" in Videos/Recordings.
pub fn new_file(video: bool) -> Option<PathBuf> {
    let (kind, fallback, folder, name, ext) = if video {
        (glib::UserDirectory::Videos, "Videos", "Recordings", "Recording", "mp4")
    } else {
        (glib::UserDirectory::Pictures, "Pictures", "Screenshots", "Screenshot", "png")
    };
    let dir = glib::user_special_dir(kind).unwrap_or_else(|| glib::home_dir().join(fallback)).join(folder);
    fs::create_dir_all(&dir).ok()?;
    let time = glib::DateTime::now_local().and_then(|t| t.format("%Y-%m-%d %H-%M-%S")).ok()?;
    // two in the same second: the later one gets a number
    (1..100)
        .map(|n| if n == 1 { format!("{name} {time}.{ext}") } else { format!("{name} {time} ({n}).{ext}") })
        .map(|file| dir.join(file))
        .find(|path| !path.exists())
}

/// A file's place as it is shown to the user: from the home directory on.
pub fn shown(path: &std::path::Path) -> String {
    path.strip_prefix(glib::home_dir()).unwrap_or(path).display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ppm_is_read() {
        let mut file = b"P6\n2 1\n255\n".to_vec();
        file.extend([1, 2, 3, 4, 5, 6]);
        assert_eq!(parse_ppm(&file), Some((2, 1, &[1u8, 2, 3, 4, 5, 6][..])));
        // a pixel value that looks like white space must not move the start
        let mut file = b"P6 1 1 255\n".to_vec();
        file.extend([b'\n', b' ', 7]);
        assert_eq!(parse_ppm(&file), Some((1, 1, &[b'\n', b' ', 7][..])));
    }

    #[test]
    fn broken_ppm_is_refused() {
        assert_eq!(parse_ppm(b""), None);
        assert_eq!(parse_ppm(b"P5\n1 1\n255\n\0"), None);
        assert_eq!(parse_ppm(b"P6\n2 2\n255\n123"), None);
        assert_eq!(parse_ppm(b"P6\n0 2\n255\n"), None);
    }

    #[test]
    fn rect_between_points() {
        let r = Rect::between((30.4, 50.0), (10.0, 20.6), 100.0, 100.0);
        assert_eq!(r, Rect { x: 10.0, y: 21.0, w: 20.0, h: 29.0 });
        // dragged off the screen
        let r = Rect::between((90.0, 90.0), (140.0, -5.0), 100.0, 100.0);
        assert_eq!(r, Rect { x: 90.0, y: 0.0, w: 10.0, h: 90.0 });
    }

    #[test]
    fn part_stays_inside_the_picture() {
        let surface = ImageSurface::create(Format::Rgb24, 200, 100).unwrap();
        let shot = Shot { surface, width: 200, height: 100 };
        let r = Rect { x: 10.0, y: 5.0, w: 20.0, h: 10.0 };
        assert_eq!(shot.part(r, 2.0), PixelRect { x: 20, y: 10, w: 40, h: 20 });
        let r = Rect { x: 90.0, y: 40.0, w: 50.0, h: 50.0 };
        assert_eq!(shot.part(r, 2.0), PixelRect { x: 180, y: 80, w: 20, h: 20 });
        assert_eq!(shot.cut(shot.all()).map(|p| (p.width(), p.height())), Some((200, 100)));
    }

    #[test]
    fn shape_is_cut_out() {
        let surface = ImageSurface::create(Format::Rgb24, 100, 100).unwrap();
        {
            let cr = cairo::Context::new(&surface).unwrap();
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.paint().unwrap();
        }
        let shot = Shot { surface, width: 100, height: 100 };
        // a triangle: its box is 40 x 30, its left bottom corner is outside it
        let mut part = shot.cut_shape(&[(10.0, 10.0), (50.0, 10.0), (50.0, 40.0)]).unwrap();
        assert_eq!((part.width(), part.height()), (40, 30));
        let stride = part.stride() as usize;
        let data = part.data().unwrap();
        assert_eq!(data[2 * stride + 35 * 4 + 3], 255, "inside the shape");
        assert_eq!(data[27 * stride + 3 * 4 + 3], 0, "outside the shape");
        drop(data);
        assert!(shot.cut_shape(&[(1.0, 1.0), (5.0, 5.0)]).is_none());
    }
}
