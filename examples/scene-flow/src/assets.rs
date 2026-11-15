//! A small offline resource encoded in the same format decoded by the worker.

pub fn demo_resource() -> Vec<u8> {
    let width = 32_u16;
    let height = 32_u16;
    let points = [
        (0.25_f32, 0.15_f32, 0.55_f32, [65_u8, 180, 245, 255]),
        (0.75, 0.3, 0.35, [80, 210, 190, 255]),
        (1.25, 0.5, 0.65, [235, 175, 75, 255]),
        (1.75, 0.7, 0.3, [200, 105, 235, 255]),
        (2.25, 0.85, 0.55, [235, 110, 130, 255]),
        (2.75, 0.5, 0.45, [75, 195, 245, 255]),
    ];
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"MSF1");
    bytes.extend_from_slice(&3_f32.to_le_bytes());
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&(points.len() as u32).to_le_bytes());
    for y in 0..height {
        for x in 0..width {
            let light = ((x / 4 + y / 4) % 2) == 0;
            bytes.extend_from_slice(if light {
                &[35, 90, 145, 255]
            } else {
                &[15, 42, 77, 255]
            });
        }
    }
    for (at, x, y, color) in points {
        bytes.extend_from_slice(&at.to_le_bytes());
        bytes.extend_from_slice(&x.to_le_bytes());
        bytes.extend_from_slice(&y.to_le_bytes());
        bytes.extend_from_slice(&color);
    }
    bytes
}
