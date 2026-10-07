//! A window's picture, as grim hands it over (PPM), cut down to the window
//! and made small.

/// A picture as rows of red, green and blue bytes.
#[derive(Debug, PartialEq)]
pub struct Rgb {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}

/// A part of a picture: left, top, width, height.
pub type Part = (usize, usize, usize, usize);

/// The widest shadow a window is taken to have.
const SHADOW_MAX: usize = 64;

/// Read a binary PPM ("P6", 255 levels).
pub fn parse_ppm(file: &[u8]) -> Option<Rgb> {
    let mut at = 0;
    let mut field = || {
        // (a header has no comments when grim writes it)
        while file.get(at)?.is_ascii_whitespace() {
            at += 1;
        }
        let from = at;
        while !file.get(at)?.is_ascii_whitespace() {
            at += 1;
        }
        std::str::from_utf8(&file[from..at]).ok()
    };
    if field()? != "P6" {
        return None;
    }
    let width: usize = field()?.parse().ok()?;
    let height: usize = field()?.parse().ok()?;
    if field()? != "255" || width == 0 || height == 0 {
        return None;
    }
    let data = file.get(at + 1..at + 1 + width.checked_mul(height)?.checked_mul(3)?)?;
    Some(Rgb { width, height, data: data.to_vec() })
}

/// Where the window is in its picture. A window that draws its own frame
/// (every GTK4 app) has room for its shadow around it, and that room comes
/// out black. It is taken away when it looks like one: black on all four
/// sides, as wide left as right, and no wider than a shadow is. Anything else
/// (a terminal with a black background) is left whole.
pub fn window_box(picture: &Rgb) -> Part {
    let black = |x: usize, y: usize| picture.data[(y * picture.width + x) * 3..][..3] == [0, 0, 0];
    let row = |y: &usize| (0..picture.width).all(|x| black(x, *y));
    let column = |x: &usize| (0..picture.height).all(|y| black(*x, y));
    let top = (0..picture.height).take_while(row).count();
    let bottom = (0..picture.height).rev().take_while(row).count();
    let left = (0..picture.width).take_while(column).count();
    let right = (0..picture.width).rev().take_while(column).count();
    let shadow = |side: usize| (1..=SHADOW_MAX).contains(&side);
    let some_left = left + right < picture.width && top + bottom < picture.height;
    if some_left && [top, bottom, left, right].into_iter().all(shadow) && left.abs_diff(right) <= 1 {
        (left, top, picture.width - left - right, picture.height - top - bottom)
    } else {
        (0, 0, picture.width, picture.height)
    }
}

/// The size a picture gets to fit into `max_width` x `max_height`. It keeps
/// its shape and is never made larger than it is.
pub fn fit(width: usize, height: usize, max_width: usize, max_height: usize) -> (usize, usize) {
    let scale = (max_width as f64 / width as f64).min(max_height as f64 / height as f64).min(1.0);
    (((width as f64 * scale).round() as usize).max(1), ((height as f64 * scale).round() as usize).max(1))
}

/// Make a part of a picture smaller: every new pixel is the average of the old
/// ones it covers, which keeps small text readable as grey lines instead of
/// noise.
pub fn shrink(picture: &Rgb, part: Part, width: usize, height: usize) -> Rgb {
    let (x0, y0, part_width, part_height) = part;
    let (width, height) = (width.clamp(1, part_width), height.clamp(1, part_height));
    let mut data = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        let top = y * part_height / height;
        let bottom = ((y + 1) * part_height / height).max(top + 1);
        for x in 0..width {
            let left = x * part_width / width;
            let right = ((x + 1) * part_width / width).max(left + 1);
            let mut sum = [0u32; 3];
            for row in y0 + top..y0 + bottom {
                let line = &picture.data[(row * picture.width + x0 + left) * 3..(row * picture.width + x0 + right) * 3];
                for pixel in line.as_chunks::<3>().0 {
                    sum[0] += u32::from(pixel[0]);
                    sum[1] += u32::from(pixel[1]);
                    sum[2] += u32::from(pixel[2]);
                }
            }
            let count = ((bottom - top) * (right - left)) as u32;
            data.extend(sum.map(|s| (s / count) as u8));
        }
    }
    Rgb { width, height, data }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(width: usize, height: usize, pixel: impl Fn(usize, usize) -> [u8; 3]) -> Rgb {
        let mut data = Vec::new();
        for y in 0..height {
            for x in 0..width {
                data.extend(pixel(x, y));
            }
        }
        Rgb { width, height, data }
    }

    #[test]
    fn ppm() {
        let mut file = b"P6\n2 1\n255\n".to_vec();
        file.extend([1, 2, 3, 4, 5, 6]);
        assert_eq!(parse_ppm(&file), Some(Rgb { width: 2, height: 1, data: vec![1, 2, 3, 4, 5, 6] }));
        assert_eq!(parse_ppm(&file[..file.len() - 1]), None);
        assert_eq!(parse_ppm(b"P5\n2 1\n255\nxx"), None);
        assert_eq!(parse_ppm(b""), None);
    }

    #[test]
    fn finds_the_window_in_its_shadow() {
        // a 6x4 window with 2 of shadow at the sides, 1 above and 3 below
        let inside = |x: usize, y: usize| (2..8).contains(&x) && (1..5).contains(&y);
        let shadowed = picture(10, 8, |x, y| if inside(x, y) { [250, 250, 251] } else { [0, 0, 0] });
        assert_eq!(window_box(&shadowed), (2, 1, 6, 4));
        // its round corners are black as well
        let round = picture(10, 8, |x, y| if inside(x, y) && (x, y) != (2, 1) { [250, 250, 251] } else { [0, 0, 0] });
        assert_eq!(window_box(&round), (2, 1, 6, 4));
        // a window without one
        assert_eq!(window_box(&picture(5, 4, |_, _| [9, 9, 9])), (0, 0, 5, 4));
        // a black terminal with some text near its left edge, and a black picture
        let terminal = picture(200, 90, |x, y| if (4..40).contains(&x) && (4..12).contains(&y) { [200, 200, 200] } else { [0, 0, 0] });
        assert_eq!(window_box(&terminal), (0, 0, 200, 90));
        assert_eq!(window_box(&picture(5, 4, |_, _| [0, 0, 0])), (0, 0, 5, 4));
    }

    #[test]
    fn fits() {
        assert_eq!(fit(1280, 720, 240, 150), (240, 135));
        assert_eq!(fit(400, 800, 240, 150), (75, 150));
        assert_eq!(fit(100, 50, 240, 150), (100, 50));
    }

    #[test]
    fn averages() {
        // a black and a white half, each 2x2
        let halves = picture(4, 2, |x, _| if x < 2 { [0, 0, 0] } else { [255, 255, 255] });
        let small = shrink(&halves, (0, 0, 4, 2), 2, 1);
        assert_eq!(small, Rgb { width: 2, height: 1, data: vec![0, 0, 0, 255, 255, 255] });
        assert_eq!(shrink(&small, (0, 0, 2, 1), 1, 1).data, vec![127, 127, 127]);
        // only a part of it
        assert_eq!(shrink(&halves, (2, 0, 2, 2), 1, 1).data, vec![255, 255, 255]);
        // not a whole number of old pixels per new one
        assert_eq!(shrink(&picture(3, 1, |_, _| [9, 9, 9]), (0, 0, 3, 1), 2, 1).data, vec![9; 6]);
    }
}
