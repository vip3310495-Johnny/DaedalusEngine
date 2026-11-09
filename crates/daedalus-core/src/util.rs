//! Small helpers reproducing JavaScript number formatting (used in messages)
//! and JavaScript integer conversions used by a few models.

/// Number#toString()
pub fn js_num_str(v: f64) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if v == 0.0 {
        return "0".into();
    }
    let a = v.abs();
    if (1e-6..1e21).contains(&a) {
        let s = format!("{}", v);
        return s;
    }
    let s = format!("{:e}", v); // e.g. 1e-7, 1.5e21
    if let Some(p) = s.find('e') {
        let (m, e) = s.split_at(p);
        let e = &e[1..];
        if e.starts_with('-') {
            return format!("{}e{}", m, e);
        }
        return format!("{}e+{}", m, e);
    }
    s
}

/// Number#toExponential(d)
pub fn js_to_exponential(v: f64, d: usize) -> String {
    if !v.is_finite() {
        return js_num_str(v);
    }
    let s = format!("{:.*e}", d, v);
    if let Some(p) = s.find('e') {
        let (m, e) = s.split_at(p);
        let e = &e[1..];
        if e.starts_with('-') {
            return format!("{}e{}", m, e);
        }
        return format!("{}e+{}", m, e);
    }
    s
}

/// Number#toPrecision(p)
pub fn js_to_precision(v: f64, p: usize) -> String {
    if !v.is_finite() {
        return js_num_str(v);
    }
    if v == 0.0 {
        if p <= 1 {
            return "0".into();
        }
        return format!("0.{}", "0".repeat(p - 1));
    }
    let e = v.abs().log10().floor() as i32;
    // rounding may bump the exponent
    let s = format!("{:.*e}", p.saturating_sub(1), v);
    let ep: i32 = s.split('e').nth(1).and_then(|x| x.parse().ok()).unwrap_or(e);
    if ep < -6 || ep >= p as i32 {
        return js_to_exponential(v, p.saturating_sub(1));
    }
    let dec = (p as i32 - 1 - ep).max(0) as usize;
    format!("{:.*}", dec, v)
}

/// ToUint32 of a JS number.
pub fn js_to_u32(v: f64) -> u32 {
    if !v.is_finite() {
        return 0;
    }
    let t = v.trunc();
    let m = t.rem_euclid(4294967296.0);
    m as u32
}

/// ToInt32 of a JS number.
pub fn js_to_i32(v: f64) -> i32 {
    js_to_u32(v) as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fmt() {
        assert_eq!(js_to_exponential(1.5, 4), "1.5000e+0");
        assert_eq!(js_to_exponential(0.0012345, 4), "1.2345e-3");
        assert_eq!(js_to_precision(0.377, 3), "0.377");
        assert_eq!(js_to_precision(6.889, 3), "6.89");
        assert_eq!(js_to_precision(1234.6, 4), "1235");
        assert_eq!(js_num_str(1e-7), "1e-7");
        assert_eq!(js_num_str(0.5), "0.5");
        assert_eq!(js_num_str(100.0), "100");
    }
}
