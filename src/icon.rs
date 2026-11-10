// Иконка приложения рисуется кодом: градиентный скруглённый квадрат как у Smart Switcher / Random Wheel,
// внутри — белое звено цепочки (ссылка). Файл без зависимостей: его подключает и build.rs (для .ico в exe),
// и само приложение (окно и значок в трее).

/// RGBA, неумноженная альфа, строки сверху вниз.
pub fn icon_rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let (a, b) = ([0x4f as f32, 0x6b as f32, 0xff as f32], [0x8a as f32, 0x5b as f32, 0xff as f32]);
    // Сглаживание ~1px, на мелких размерах чуть мягче.
    let aa = if size <= 24 { 0.75 } else { 0.5 };
    let smooth = |d: f32| (aa - d).clamp(0.0, 2.0 * aa) / (2.0 * aa);
    let (ux, uy) = (std::f32::consts::FRAC_1_SQRT_2, -std::f32::consts::FRAC_1_SQRT_2);
    let off = s * 0.125;
    let half_len = s * 0.115;
    let radius = s * 0.125;
    let thick = s * if size <= 24 { 0.05 } else { 0.042 };

    // Расстояние до капсулы (отрезок вдоль диагонали + радиус).
    let capsule = |px: f32, py: f32, cx: f32, cy: f32| -> f32 {
        let (dx, dy) = (px - cx, py - cy);
        let t = (dx * ux + dy * uy).clamp(-half_len, half_len);
        (dx - ux * t).hypot(dy - uy * t) - radius
    };

    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let r = s * 0.24;
            let qx = (px - s / 2.0).abs() - (s / 2.0 - r);
            let qy = (py - s / 2.0).abs() - (s / 2.0 - r);
            let d_box = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r;
            let cover = smooth(d_box);
            let t = (px + py) / (2.0 * s);
            let mut col = [0f32; 3];
            for i in 0..3 {
                col[i] = a[i] + (b[i] - a[i]) * t;
            }
            let (c1x, c1y) = (s / 2.0 - ux * off, s / 2.0 - uy * off);
            let (c2x, c2y) = (s / 2.0 + ux * off, s / 2.0 + uy * off);
            let ring1 = capsule(px, py, c1x, c1y).abs() - thick;
            let ring2 = capsule(px, py, c2x, c2y).abs() - thick;
            let white = smooth(ring1).max(smooth(ring2));
            for c in &mut col {
                *c += (255.0 - *c) * white;
            }
            rgba.extend_from_slice(&[col[0] as u8, col[1] as u8, col[2] as u8, (cover * 255.0) as u8]);
        }
    }
    rgba
}

/// Файл .ico из нескольких размеров (BMP-записи с альфой).
#[allow(dead_code)]
pub fn ico_file(sizes: &[u32]) -> Vec<u8> {
    let images: Vec<Vec<u8>> = sizes.iter().map(|&n| bmp_entry(n)).collect();
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&n, img) in sizes.iter().zip(&images) {
        let dim = if n >= 256 { 0 } else { n as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(img.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += img.len() as u32;
    }
    for img in images {
        out.extend_from_slice(&img);
    }
    out
}

fn bmp_entry(n: u32) -> Vec<u8> {
    let rgba = icon_rgba(n);
    let mask_row = n.div_ceil(32) * 4;
    let mut out = Vec::new();
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(n as i32).to_le_bytes());
    out.extend_from_slice(&(2 * n as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(n * n * 4 + mask_row * n).to_le_bytes());
    out.extend_from_slice(&[0; 16]);
    for y in (0..n).rev() {
        for x in 0..n {
            let i = ((y * n + x) * 4) as usize;
            out.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
        }
    }
    out.extend(std::iter::repeat_n(0u8, (mask_row * n) as usize));
    out
}
