//! Exact conversions between `f64` and [`Decimal`], through decimal text.
//!
//! `rust_decimal`'s own conversions are not exact both ways (issue #366):
//!
//! - `Decimal::try_from(f64)` keeps about 15 significant digits, so most
//!   doubles do not survive the trip (`906404.2095968979` becomes
//!   `906404.209596898`).
//! - `Decimal::to_f64` can land one unit in the last place off
//!   (`225018.77906698454` comes back as `225018.77906698457`).
//!
//! Both functions here go through the decimal text instead: [`f64_to_decimal`]
//! takes a double's shortest round-trip digits, [`decimal_to_f64`] parses a
//! decimal's digits correctly rounded. A double that goes through both comes
//! back with the same bits.
//!
//! ```
//! use kernel::{decimal_to_f64, f64_to_decimal};
//!
//! let exact = f64_to_decimal(906404.2095968979).unwrap();
//! assert_eq!(exact.to_string(), "906404.2095968979");
//! assert_eq!(decimal_to_f64(exact), 906404.2095968979);
//!
//! assert_eq!(f64_to_decimal(1e30), None);
//! assert_eq!(f64_to_decimal(1.5e-30), None);
//! ```

use std::fmt::{self, Write};

use rust_decimal::Decimal;

/// The [`Decimal`] holding exactly `value`'s shortest round-trip digits
/// (what Postgres writes for a `float8`). `None` when no `Decimal` holds them:
/// past ±7.9e28, more than 28 fractional digits, or not finite.
pub fn f64_to_decimal(value: f64) -> Option<Decimal> {
    if !value.is_finite() {
        return None;
    }
    let mut text = Text::default();
    write!(text, "{value}").ok()?;
    Decimal::from_str_exact(text.as_str()).ok()
}

/// The `f64` nearest `value`, correctly rounded; `0.0` for a zero of either
/// sign.
pub fn decimal_to_f64(value: Decimal) -> f64 {
    if value.is_zero() {
        return 0.0;
    }
    if let Some(exact) = fast_quotient(value) {
        return exact;
    }
    let mut text = Text::default();
    match write!(text, "{value}") {
        Ok(()) => text.as_str().parse().unwrap_or(f64::NAN),
        Err(_) => value.to_string().parse().unwrap_or(f64::NAN),
    }
}

/// Powers of ten an `f64` holds exactly.
const EXACT_POWERS_OF_TEN: [f64; 23] = [
    1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16,
    1e17, 1e18, 1e19, 1e20, 1e21, 1e22,
];

/// `value` as one IEEE division of two exact `f64`s, which rounds correctly:
/// when the mantissa is at most 2^53 and the scale at most 22. `None`
/// otherwise.
#[allow(clippy::cast_precision_loss)]
fn fast_quotient(value: Decimal) -> Option<f64> {
    let mantissa = value.mantissa();
    if mantissa.unsigned_abs() > 1 << 53 {
        return None;
    }
    let power = usize::try_from(value.scale()).ok()?;
    EXACT_POWERS_OF_TEN
        .get(power)
        .map(|power| mantissa as f64 / power)
}

/// Number text on the stack. 64 bytes hold every `Decimal` (a sign, 29 digits,
/// a point and up to 28 leading fractional zeros) and every `f64` a `Decimal`
/// can hold; a longer text is an `Err`, and no `Decimal` holds it.
struct Text {
    bytes: [u8; 64],
    len: usize,
}

impl Default for Text {
    fn default() -> Self {
        Self {
            bytes: [0; 64],
            len: 0,
        }
    }
}

impl Text {
    fn as_str(&self) -> &str {
        self.bytes
            .get(..self.len)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .unwrap_or_default()
    }
}

impl Write for Text {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let end = self.len + s.len();
        let slot = self.bytes.get_mut(self.len..end).ok_or(fmt::Error)?;
        slot.copy_from_slice(s.as_bytes());
        self.len = end;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::str::FromStr;

    use super::*;
    use proptest::prelude::*;
    use rust_decimal::prelude::ToPrimitive;

    #[test]
    fn rust_decimals_own_conversions_are_not_exact() {
        assert_eq!(
            Decimal::try_from(906_404.209_596_897_9)
                .unwrap()
                .to_string(),
            "906404.209596898"
        );
        let long = Decimal::from_str("225018.77906698454").unwrap();
        assert_ne!(long.to_f64(), Some(225_018.779_066_984_54));
        assert_eq!(decimal_to_f64(long), 225_018.779_066_984_54);
    }

    #[test]
    fn edges() {
        assert_eq!(f64_to_decimal(0.1).unwrap().to_string(), "0.1");
        assert_eq!(
            f64_to_decimal(1e28).unwrap().to_string(),
            "10000000000000000000000000000"
        );
        assert_eq!(
            f64_to_decimal(1e-28).unwrap().to_string(),
            "0.0000000000000000000000000001"
        );
        assert_eq!(f64_to_decimal(1e29), None);
        assert_eq!(f64_to_decimal(f64::NAN), None);
        assert_eq!(f64_to_decimal(f64::MAX), None);
        assert_eq!(f64_to_decimal(f64::MIN_POSITIVE), None);
        assert_eq!(decimal_to_f64(Decimal::ZERO), 0.0);
        assert_eq!(decimal_to_f64(Decimal::MAX), 7.922_816_251_426_434e28);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(4096))]

        #[test]
        fn a_double_survives_the_round_trip(bits in any::<u64>()) {
            let value = f64::from_bits(bits);
            if let Some(exact) = f64_to_decimal(value) {
                prop_assert_eq!(decimal_to_f64(exact).to_bits(), (value + 0.0).to_bits());
                prop_assert_eq!(exact.to_string().parse::<f64>().unwrap(), value);
            }
        }

        #[test]
        fn the_fast_quotient_is_the_nearest_double(mantissa in any::<i64>(), scale in 0u32..=28) {
            let value = Decimal::new(mantissa, scale);
            let nearest: f64 = value.to_string().parse().unwrap();
            prop_assert_eq!(decimal_to_f64(value).to_bits(), (nearest + 0.0).to_bits());
        }

        #[test]
        fn an_everyday_double_is_held_exactly(value in -1e9f64..1e9) {
            let exact = f64_to_decimal(value).unwrap();
            prop_assert_eq!(decimal_to_f64(exact).to_bits(), (value + 0.0).to_bits());
        }
    }
}
