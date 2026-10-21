//! Bounded geographic geometry shared by every installed WebAssembly package.
use std::f32::consts::PI;
use wasm_bindgen::prelude::*;

type Point = [f32; 3];
fn dot(a: Point, b: Point) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
fn cross(a: Point, b: Point) -> Point {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn normal(p: Point) -> Point {
    let length = dot(p, p).sqrt();
    p.map(|v| v / length)
}
fn point(lat: f32, lon: f32) -> Point {
    let (lat, lon) = (lat.to_radians(), lon.to_radians());
    [lat.cos() * lon.sin(), lat.sin(), lat.cos() * lon.cos()]
}
fn coordinates(lat: f32, lon: f32) -> bool {
    lat.is_finite()
        && lon.is_finite()
        && (-90.0..=90.0).contains(&lat)
        && (-180.0..=180.0).contains(&lon)
}
fn tangent(p: Point) -> Point {
    normal(cross(
        p,
        if p[1].abs() < 0.9 {
            [0.0, 1.0, 0.0]
        } else {
            [1.0, 0.0, 0.0]
        },
    ))
}
// Coarse illustrative coast silhouettes, bundled data; never geolocation evidence.
const LAND: &[&[(f32, f32)]] = &[
    &[
        (-168., 70.),
        (-130., 72.),
        (-108., 81.),
        (-60., 52.),
        (-80., 25.),
        (-100., 16.),
        (-117., 32.),
        (-135., 55.),
    ],
    &[
        (-81., 12.),
        (-48., 0.),
        (-35., -8.),
        (-52., -35.),
        (-69., -55.),
        (-79., -8.),
    ],
    &[(-53., 60.), (-20., 80.), (-52., 84.), (-64., 68.)],
    &[
        (-17., 36.),
        (10., 37.),
        (35., 30.),
        (51., 11.),
        (42., -12.),
        (19., -35.),
        (9., -18.),
        (-10., 5.),
    ],
    &[
        (-10., 36.),
        (-9., 60.),
        (28., 71.),
        (100., 77.),
        (177., 65.),
        (158., 48.),
        (135., 35.),
        (120., 18.),
        (105., 1.),
        (77., 8.),
        (62., 26.),
        (43., 12.),
        (32., 31.),
    ],
    &[
        (113., -22.),
        (131., -12.),
        (153., -26.),
        (146., -40.),
        (116., -34.),
    ],
    &[(47., -13.), (51., -17.), (48., -26.), (44., -24.)],
    &[(166., -34.), (179., -39.), (169., -47.), (165., -45.)],
];
fn land(lat: f32, lon: f32) -> f32 {
    if lat < -72.0 {
        return 1.0;
    }
    for polygon in LAND {
        let mut inside = false;
        let mut j = polygon.len() - 1;
        for (i, &(x, y)) in polygon.iter().enumerate() {
            let (xj, yj) = polygon[j];
            if ((y > lat) != (yj > lat)) && lon < (xj - x) * (lat - y) / (yj - y) + x {
                inside = !inside;
            }
            j = i;
        }
        if inside {
            return 1.0;
        }
    }
    0.0
}

/// Interleaved xyz + land mask. Fixed upper bounds keep mobile GPU memory finite.
#[wasm_bindgen]
pub fn globe_mesh(rows: u32, columns: u32) -> Result<Vec<f32>, JsValue> {
    if !(8..=64).contains(&rows) || !(16..=128).contains(&columns) {
        return Err(JsValue::from_str("globe mesh bounds"));
    }
    let mut vertices = Vec::with_capacity((rows * columns * 24) as usize);
    for row in 0..rows {
        for column in 0..columns {
            for (dr, dc) in [(0, 0), (1, 0), (0, 1), (0, 1), (1, 0), (1, 1)] {
                let lat = -90.0 + 180.0 * (row + dr) as f32 / rows as f32;
                let lon = -180.0 + 360.0 * (column + dc) as f32 / columns as f32;
                vertices.extend(point(lat, lon));
                vertices.push(land(lat, lon));
            }
        }
    }
    Ok(vertices)
}

fn route(start: Point, end: Point, segments: u32) -> Vec<f32> {
    let cosine = dot(start, end).clamp(-1.0, 1.0);
    let angle = cosine.acos();
    let projected = std::array::from_fn(|i| end[i] - cosine * start[i]);
    let axis = if dot(projected, projected) < 1e-10 {
        tangent(start)
    } else {
        normal(projected)
    };
    let mut vertices = Vec::with_capacity((segments as usize + 1) * 3);
    for index in 0..=segments {
        let fraction = index as f32 / segments as f32;
        let (sine, cosine) = (angle * fraction).sin_cos();
        let height = 1.012 + 0.22 * (PI * fraction).sin();
        vertices.extend((0..3).map(|i| height * (start[i] * cosine + axis[i] * sine)));
    }
    vertices
}

#[wasm_bindgen]
pub fn globe_route(
    from_lat: f32,
    from_lon: f32,
    to_lat: f32,
    to_lon: f32,
    segments: u32,
) -> Result<Vec<f32>, JsValue> {
    if !coordinates(from_lat, from_lon)
        || !coordinates(to_lat, to_lon)
        || !(8..=128).contains(&segments)
    {
        return Err(JsValue::from_str("route coordinates or bounds"));
    }
    Ok(route(
        point(from_lat, from_lon),
        point(to_lat, to_lon),
        segments,
    ))
}

#[wasm_bindgen]
pub fn globe_ripple(lat: f32, lon: f32, radius: f32, segments: u32) -> Result<Vec<f32>, JsValue> {
    if !coordinates(lat, lon)
        || !radius.is_finite()
        || !(0.0..=45.0).contains(&radius)
        || !(16..=128).contains(&segments)
    {
        return Err(JsValue::from_str("ripple coordinates or bounds"));
    }
    let center = point(lat, lon);
    let x = tangent(center);
    let y = cross(center, x);
    let (sine, cosine) = radius.to_radians().sin_cos();
    let mut vertices = Vec::with_capacity((segments as usize + 1) * 3);
    for index in 0..=segments {
        let (s, c) = (2.0 * PI * index as f32 / segments as f32).sin_cos();
        vertices
            .extend((0..3).map(|i| 1.014 * (center[i] * cosine + sine * (x[i] * c + y[i] * s))));
    }
    Ok(vertices)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mesh_has_bounded_unit_sphere_vertices() {
        let mesh = globe_mesh(8, 16).unwrap();
        assert_eq!(mesh.len(), 8 * 16 * 24);
        for v in mesh.as_chunks::<4>().0 {
            assert!((dot([v[0], v[1], v[2]], [v[0], v[1], v[2]]) - 1.0).abs() < 1e-5);
            assert!([0.0, 1.0].contains(&v[3]));
        }
    }
    #[test]
    fn coincident_and_antipodal_routes_remain_finite() {
        for destination in [[1., 0., 0.], [-1., 0., 0.], [0., 1., 0.]] {
            let vertices = route([1., 0., 0.], destination, 64);
            assert_eq!(vertices.len(), 195);
            assert!(vertices.iter().all(|value| value.is_finite()));
            for i in 0..3 {
                assert!((vertices[192 + i] - destination[i] * 1.012).abs() < 1e-4);
            }
        }
    }
    #[test]
    fn ripple_closes_at_poles_and_equator() {
        for latitude in [-90., 0., 90.] {
            let vertices = globe_ripple(latitude, 0., 15., 64).unwrap();
            for i in 0..3 {
                assert!((vertices[i] - vertices[192 + i]).abs() < 1e-5);
            }
            assert!(vertices.iter().all(|value| value.is_finite()));
        }
        assert!(!coordinates(f32::NAN, 0.));
        assert!(!coordinates(0., 181.));
    }
}
