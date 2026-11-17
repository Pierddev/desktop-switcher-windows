//! A small icon drawn in code: two stacked desktops with arrows.

/// RGBA pixels, row by row from the top.
pub fn rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let mut px = vec![0u8; (size * size * 4) as usize];
    let back = [0x5b, 0x8d, 0xd6];
    let front = [0x2b, 0x4c, 0x7e];
    let white = [0xff, 0xff, 0xff];
    let rect = |x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32| -> bool {
        if x < x0 || x > x1 || y < y0 || y > y1 {
            return false;
        }
        let cx = x.clamp(x0 + r, x1 - r);
        let cy = y.clamp(y0 + r, y1 - r);
        (x - cx).powi(2) + (y - cy).powi(2) <= r * r
    };
    for j in 0..size {
        for i in 0..size {
            let x = (i as f32 + 0.5) / s;
            let y = (j as f32 + 0.5) / s;
            let mut c: Option<[u8; 3]> = None;
            if rect(x, y, 0.30, 0.08, 0.96, 0.62, 0.08) {
                c = Some(back);
            }
            if rect(x, y, 0.04, 0.30, 0.74, 0.92, 0.08) {
                c = Some(front);
                // left and right arrows inside the front desktop
                let ay = (y - 0.61).abs();
                if x > 0.12 && x < 0.30 && ay < (x - 0.12) * 0.75 {
                    c = Some(white);
                }
                if x > 0.48 && x < 0.66 && ay < (0.66 - x) * 0.75 {
                    c = Some(white);
                }
            }
            if let Some(c) = c {
                let k = ((j * size + i) * 4) as usize;
                px[k..k + 3].copy_from_slice(&c);
                px[k + 3] = 255;
            }
        }
    }
    px
}
