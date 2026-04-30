pub fn trilinear_value(values: [f64; 8], u: [f64; 3]) -> f64 {
    let [x, y, z] = u;
    let c00 = values[0] * (1.0 - x) + values[1] * x;
    let c10 = values[2] * (1.0 - x) + values[3] * x;
    let c01 = values[4] * (1.0 - x) + values[5] * x;
    let c11 = values[6] * (1.0 - x) + values[7] * x;
    let c0 = c00 * (1.0 - y) + c10 * y;
    let c1 = c01 * (1.0 - y) + c11 * y;
    c0 * (1.0 - z) + c1 * z
}

pub fn trilinear_gradient(values: [f64; 8], u: [f64; 3]) -> [f64; 3] {
    let [x, y, z] = u;

    let dx00 = values[1] - values[0];
    let dx10 = values[3] - values[2];
    let dx01 = values[5] - values[4];
    let dx11 = values[7] - values[6];
    let dx0 = dx00 * (1.0 - y) + dx10 * y;
    let dx1 = dx01 * (1.0 - y) + dx11 * y;

    let dy00 = values[2] - values[0];
    let dy10 = values[3] - values[1];
    let dy01 = values[6] - values[4];
    let dy11 = values[7] - values[5];
    let dy0 = dy00 * (1.0 - x) + dy10 * x;
    let dy1 = dy01 * (1.0 - x) + dy11 * x;

    let dz00 = values[4] - values[0];
    let dz10 = values[5] - values[1];
    let dz01 = values[6] - values[2];
    let dz11 = values[7] - values[3];
    let dz0 = dz00 * (1.0 - x) + dz10 * x;
    let dz1 = dz01 * (1.0 - x) + dz11 * x;

    [
        dx0 * (1.0 - z) + dx1 * z,
        dy0 * (1.0 - z) + dy1 * z,
        dz0 * (1.0 - y) + dz1 * y,
    ]
}
