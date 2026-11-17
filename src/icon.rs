//! Application icon: a light blue rounded square with a smaller blue one
//! overlapping it, drawn once and reused for the executable, the tray and
//! the settings window.

/// RGBA pixels, row by row from the top.
pub fn rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let feather = 1.0 / s;
    let light = [0xa8, 0xc5, 0xf5];
    let blue = [0x2f, 0x6f, 0xeb];
    let mut px = vec![0u8; (size * size * 4) as usize];

    for j in 0..size {
        for i in 0..size {
            let x = (i as f32 + 0.5) / s;
            let y = (j as f32 + 0.5) / s;
            let back = rounded_rect(x, y, 0.10, 0.10, 0.90, 0.90, 0.20, feather);
            let front = rounded_rect(x, y, 0.22, 0.34, 0.64, 0.76, 0.12, feather);
            let alpha = front + back * (1.0 - front);
            if alpha <= 0.0 {
                continue;
            }
            let w_back = back * (1.0 - front) / alpha;
            let w_front = front / alpha;
            let k = ((j * size + i) * 4) as usize;
            for c in 0..3 {
                px[k + c] = (light[c] as f32 * w_back + blue[c] as f32 * w_front).round() as u8;
            }
            px[k + 3] = (alpha * 255.0).round() as u8;
        }
    }
    px
}

/// Coverage (0..=1) of a rounded rectangle in normalised coordinates.
fn rounded_rect(
    x: f32,
    y: f32,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    r: f32,
    feather: f32,
) -> f32 {
    let cx = (x0 + x1) * 0.5;
    let cy = (y0 + y1) * 0.5;
    let hx = (x1 - x0) * 0.5;
    let hy = (y1 - y0) * 0.5;
    let qx = (x - cx).abs() - (hx - r);
    let qy = (y - cy).abs() - (hy - r);
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    let d = outside + qx.max(qy).min(0.0) - r;
    (0.5 - d / feather).clamp(0.0, 1.0)
}
