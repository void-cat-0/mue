use std::f64::consts::{FRAC_PI_2, PI, TAU};

#[derive(Debug)]
pub(super) enum Ease {
    Linear,
    Rpe { kind: u32, left: f64, right: f64 },
    Bezier([f64; 4]),
}

impl Ease {
    pub fn at(&self, t: f64) -> f64 {
        if t == 0. || t == 1. {
            return t;
        }
        match *self {
            Self::Linear => t,
            Self::Rpe { kind, left, right } => {
                let low = curve(kind, left);
                let span = curve(kind, right) - low;
                if span.abs() < 1e-12 {
                    t
                } else {
                    (curve(kind, left + (right - left) * t) - low) / span
                }
            }
            Self::Bezier([x1, y1, x2, y2]) => {
                // Resolve the parameter against the x component, then sample y.
                let cubic = |p, a, b| {
                    let q: f64 = 1. - p;
                    p * (3. * q * (q * a + p * b) + p * p)
                };
                let (mut lo, mut hi) = (0., 1.);
                for _ in 0..24 {
                    let mid = (lo + hi) * 0.5;
                    if cubic(mid, x1, x2) < t {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                cubic((lo + hi) * 0.5, y1, y2)
            }
        }
    }
}

fn curve(kind: u32, t: f64) -> f64 {
    if t == 0. || t == 1. {
        return t;
    }
    let out_power = |n| 1. - (1. - t).powi(n);
    let in_out = |input: fn(f64) -> f64| {
        if t < 0.5 {
            input(2. * t) / 2.
        } else {
            1. - input(2. - 2. * t) / 2.
        }
    };
    let circular = |s: f64| 1. - (1. - s * s).sqrt();
    let back = |s: f64| s * s * (2.70158 * s - 1.70158);
    match kind {
        2 => (t * FRAC_PI_2).sin(),
        3 => 1. - (t * FRAC_PI_2).cos(),
        4 => out_power(2),
        5 => t * t,
        6 => (1. - (t * PI).cos()) * 0.5,
        7 => in_out(|s| s * s),
        8 => out_power(3),
        9 => t.powi(3),
        10 => out_power(4),
        11 => t.powi(4),
        12 => in_out(|s| s.powi(3)),
        13 => in_out(|s| s.powi(4)),
        14 => out_power(5),
        15 => t.powi(5),
        16 => 1. - (-10. * t).exp2(),
        17 => (10. * (t - 1.)).exp2(),
        18 => 1. - circular(1. - t),
        19 => circular(t),
        20 => 1. - back(1. - t),
        21 => back(t),
        22 => in_out(circular),
        23 => in_out(back),
        24 => elastic_out(t),
        25 => 1. - elastic_out(1. - t),
        26 => bounce(t),
        27 => 1. - bounce(1. - t),
        28 => in_out(|s| 1. - bounce(1. - s)),
        29 => in_out(|s| 1. - elastic_out(1. - s)),
        _ => t,
    }
}

fn elastic_out(t: f64) -> f64 {
    1. - (-10. * t).exp2() * ((t * 10.) * TAU / 3.).cos()
}

fn bounce(t: f64) -> f64 {
    let (center, floor) = if t < 4. / 11. {
        (0., 0.)
    } else if t < 8. / 11. {
        (6. / 11., 0.75)
    } else if t < 10. / 11. {
        (9. / 11., 0.9375)
    } else {
        (10.5 / 11., 0.984375)
    };
    floor + (2.75 * (t - center)).powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_easing_rescales_endpoints_without_clamping_overshoot() {
        let clipped = Ease::Rpe {
            kind: 5,
            left: 0.25,
            right: 0.75,
        };
        assert_eq!(clipped.at(0.), 0.);
        assert_eq!(clipped.at(1.), 1.);
        assert_eq!(clipped.at(0.5), 0.375);
        assert!(
            Ease::Rpe {
                kind: 24,
                left: 0.,
                right: 1.
            }
            .at(0.15)
                > 1.
        );
    }

    #[test]
    fn bezier_inverts_the_x_axis() {
        let cubic = Ease::Bezier([0.2, 0.2, 0.3, 0.3]);
        assert!((cubic.at(0.5) - 0.5).abs() < 1e-7);
    }
}
