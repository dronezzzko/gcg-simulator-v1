//! Platform-libm results that CPython's `math` module and `**` operator return.
//!
//! CPython calls the C library for `math.erfc`, `math.exp`, `math.log` and `x ** y`. Rust's
//! `f64::exp`/`ln` lower to the same libm calls, but two cases need care:
//! - `x ** 2.0` is libm `pow`, which differs from `x * x` in the last bit for about 0.1% of
//!   inputs on Apple libm; LLVM rewrites a constant `powf(2.0)` into `x * x`, so the exponent
//!   goes through `black_box`.
//! - `f64::erfc` is unstable, and the pure-Rust `libm` crate disagrees with Apple's `erfc` on
//!   about a quarter of inputs, so `erfc` is declared from the system C library.

#[allow(unsafe_code)]
mod ffi {
    // SAFETY: `double erfc(double)` from the platform C math library (libSystem on macOS,
    // libm elsewhere) is a pure function of one `f64` with no pointers, global state or
    // preconditions; the declared signature matches C99 7.12.8.3 exactly, so every call is
    // sound.
    unsafe extern "C" {
        pub safe fn erfc(x: f64) -> f64;
    }
}

/// `math.erfc(x)`.
#[must_use]
pub fn erfc(x: f64) -> f64 {
    ffi::erfc(x)
}

/// `x ** y` for floats (libm `pow`, never strength-reduced).
#[must_use]
pub fn pow(x: f64, y: f64) -> f64 {
    x.powf(std::hint::black_box(y))
}

/// `x ** 2` for a float.
#[must_use]
pub fn square(x: f64) -> f64 {
    pow(x, 2.0)
}

/// `math.exp(x)`.
#[must_use]
pub fn exp(x: f64) -> f64 {
    x.exp()
}

/// `math.log(x)` for `x > 0`.
#[must_use]
pub fn ln(x: f64) -> f64 {
    x.ln()
}

/// `math.sqrt(x)` (correctly rounded everywhere).
#[must_use]
pub fn sqrt(x: f64) -> f64 {
    x.sqrt()
}
