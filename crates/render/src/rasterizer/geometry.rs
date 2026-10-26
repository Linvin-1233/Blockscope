//! Small geometry helpers shared by projection, binning, and tile rasterization.
pub(super) fn intersects(bbox: &[f64; 4], x0: u32, y0: u32, width: u32, height: u32) -> bool {
    let max_x = f64::from(x0 + width);
    let max_y = f64::from(y0 + height);
    bbox[2] >= f64::from(x0) && bbox[0] < max_x && bbox[3] >= f64::from(y0) && bbox[1] < max_y
}

pub(super) fn bounds(points: &[[f64; 3]; 3]) -> [f64; 4] {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for point in points {
        min_x = min_x.min(point[0]);
        max_x = max_x.max(point[0]);
        min_y = min_y.min(point[1]);
        max_y = max_y.max(point[1]);
    }

    [min_x, min_y, max_x, max_y]
}

pub(super) fn quad_bounds(points: &[[f64; 3]; 4]) -> [f64; 4] {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for point in points {
        min_x = min_x.min(point[0]);
        max_x = max_x.max(point[0]);
        min_y = min_y.min(point[1]);
        max_y = max_y.max(point[1]);
    }

    [min_x, min_y, max_x, max_y]
}

pub(super) fn edge(a: [f64; 3], b: [f64; 3], point: [f64; 3]) -> f64 {
    (b[0] - a[0]) * (point[1] - a[1]) - (b[1] - a[1]) * (point[0] - a[0])
}

pub(super) fn dot3(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

pub(super) fn to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}
