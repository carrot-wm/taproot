//! The `printf` formatting engine, walking the va.rs tag on stable Rust.
//!
//! Vendored from the printf-compat crate, version 0.4.0, by lights0123
//! (<https://github.com/lights0123/printf-compat>), licensed MIT OR
//! Apache-2.0. The port replaces the crate's nightly `VaList` argument
//! source with `&mut crate::va::VaListTag` (each `next_arg` call site
//! becomes a `tag.arg::<T>()` call), drops the `VaList`-holding `display`
//! adapter and the std-only `io_write` adapter that c-scape never used,
//! and replaces the parser's `itertools::tuple_windows` walk with a
//! peekable iterator. The parsing and formatting behavior is otherwise
//! unchanged, including its documented differences from glibc (see
//! [`output::fmt_write`]).
//!
//! This module is x86_64-only, like the walker it consumes; other
//! architectures keep formatting through the printf-compat crate itself
//! on the nightly `VaList` path.

use core::{ffi::*, fmt};

pub mod output;
mod parser;
use argument::*;
pub use parser::format;

pub mod argument {
    use super::*;

    bitflags::bitflags! {
        /// Flags field.
        ///
        /// Definitions from
        /// [Wikipedia](https://en.wikipedia.org/wiki/Printf_format_string#Flags_field).
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
        pub struct Flags: u8 {
            /// Left-align the output of this placeholder. (The default is to
            /// right-align the output.)
            const LEFT_ALIGN = 0b00000001;
            /// Prepends a plus for positive signed-numeric types. positive =
            /// `+`, negative = `-`.
            ///
            /// (The default doesn't prepend anything in front of positive
            /// numbers.)
            const PREPEND_PLUS = 0b00000010;
            /// Prepends a space for positive signed-numeric types. positive = `
            /// `, negative = `-`. This flag is ignored if the
            /// [`PREPEND_PLUS`][Flags::PREPEND_PLUS] flag exists.
            ///
            /// (The default doesn't prepend anything in front of positive
            /// numbers.)
            const PREPEND_SPACE = 0b00000100;
            /// When the 'width' option is specified, prepends zeros for numeric
            /// types. (The default prepends spaces.)
            ///
            /// For example, `printf("%4X",3)` produces `   3`, while
            /// `printf("%04X",3)` produces `0003`.
            const PREPEND_ZERO = 0b00001000;
            /// The integer or exponent of a decimal has the thousands grouping
            /// separator applied.
            const THOUSANDS_GROUPING = 0b00010000;
            /// Alternate form:
            ///
            /// For `g` and `G` types, trailing zeros are not removed. \
            /// For `f`, `F`, `e`, `E`, `g`, `G` types, the output always
            /// contains a decimal point. \ For `o`, `x`, `X` types,
            /// the text `0`, `0x`, `0X`, respectively, is prepended
            /// to non-zero numbers.
            const ALTERNATE_FORM = 0b00100000;
        }
    }

    #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
    pub enum DoubleFormat {
        /// `f`
        Normal,
        /// `F`
        UpperNormal,
        /// `e`
        Scientific,
        /// `E`
        UpperScientific,
        /// `g`
        Auto,
        /// `G`
        UpperAuto,
        /// `a`
        Hex,
        /// `A`
        UpperHex,
    }

    impl DoubleFormat {
        /// If the format is uppercase.
        pub fn is_upper(self) -> bool {
            use DoubleFormat::*;
            matches!(self, UpperNormal | UpperScientific | UpperAuto | UpperHex)
        }

        pub fn set_upper(self, upper: bool) -> Self {
            use DoubleFormat::*;
            match self {
                Normal | UpperNormal => {
                    if upper {
                        UpperNormal
                    } else {
                        Normal
                    }
                }
                Scientific | UpperScientific => {
                    if upper {
                        UpperScientific
                    } else {
                        Scientific
                    }
                }
                Auto | UpperAuto => {
                    if upper {
                        UpperAuto
                    } else {
                        Auto
                    }
                }
                Hex | UpperHex => {
                    if upper {
                        UpperHex
                    } else {
                        Hex
                    }
                }
            }
        }
    }

    #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
    #[non_exhaustive]
    pub enum SignedInt {
        Int(c_int),
        Char(c_schar),
        Short(c_short),
        Long(c_long),
        LongLong(c_longlong),
        Isize(isize),
    }

    impl From<SignedInt> for i64 {
        fn from(num: SignedInt) -> Self {
            // Some casts are only needed on some platforms.
            #[allow(clippy::unnecessary_cast)]
            match num {
                SignedInt::Int(x) => x as i64,
                SignedInt::Char(x) => x as i64,
                SignedInt::Short(x) => x as i64,
                SignedInt::Long(x) => x as i64,
                SignedInt::LongLong(x) => x as i64,
                SignedInt::Isize(x) => x as i64,
            }
        }
    }

    impl SignedInt {
        pub fn is_sign_negative(self) -> bool {
            match self {
                SignedInt::Int(x) => x < 0,
                SignedInt::Char(x) => x < 0,
                SignedInt::Short(x) => x < 0,
                SignedInt::Long(x) => x < 0,
                SignedInt::LongLong(x) => x < 0,
                SignedInt::Isize(x) => x < 0,
            }
        }
    }

    impl fmt::Display for SignedInt {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                SignedInt::Int(x) => fmt::Display::fmt(x, f),
                SignedInt::Char(x) => fmt::Display::fmt(x, f),
                SignedInt::Short(x) => fmt::Display::fmt(x, f),
                SignedInt::Long(x) => fmt::Display::fmt(x, f),
                SignedInt::LongLong(x) => fmt::Display::fmt(x, f),
                SignedInt::Isize(x) => fmt::Display::fmt(x, f),
            }
        }
    }

    #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
    #[non_exhaustive]
    pub enum UnsignedInt {
        Int(c_uint),
        Char(c_uchar),
        Short(c_ushort),
        Long(c_ulong),
        LongLong(c_ulonglong),
        Isize(usize),
    }

    impl From<UnsignedInt> for u64 {
        fn from(num: UnsignedInt) -> Self {
            // Some casts are only needed on some platforms.
            #[allow(clippy::unnecessary_cast)]
            match num {
                UnsignedInt::Int(x) => x as u64,
                UnsignedInt::Char(x) => x as u64,
                UnsignedInt::Short(x) => x as u64,
                UnsignedInt::Long(x) => x as u64,
                UnsignedInt::LongLong(x) => x as u64,
                UnsignedInt::Isize(x) => x as u64,
            }
        }
    }

    impl fmt::Display for UnsignedInt {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                UnsignedInt::Int(x) => fmt::Display::fmt(x, f),
                UnsignedInt::Char(x) => fmt::Display::fmt(x, f),
                UnsignedInt::Short(x) => fmt::Display::fmt(x, f),
                UnsignedInt::Long(x) => fmt::Display::fmt(x, f),
                UnsignedInt::LongLong(x) => fmt::Display::fmt(x, f),
                UnsignedInt::Isize(x) => fmt::Display::fmt(x, f),
            }
        }
    }

    impl fmt::LowerHex for UnsignedInt {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                UnsignedInt::Int(x) => fmt::LowerHex::fmt(x, f),
                UnsignedInt::Char(x) => fmt::LowerHex::fmt(x, f),
                UnsignedInt::Short(x) => fmt::LowerHex::fmt(x, f),
                UnsignedInt::Long(x) => fmt::LowerHex::fmt(x, f),
                UnsignedInt::LongLong(x) => fmt::LowerHex::fmt(x, f),
                UnsignedInt::Isize(x) => fmt::LowerHex::fmt(x, f),
            }
        }
    }

    impl fmt::UpperHex for UnsignedInt {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                UnsignedInt::Int(x) => fmt::UpperHex::fmt(x, f),
                UnsignedInt::Char(x) => fmt::UpperHex::fmt(x, f),
                UnsignedInt::Short(x) => fmt::UpperHex::fmt(x, f),
                UnsignedInt::Long(x) => fmt::UpperHex::fmt(x, f),
                UnsignedInt::LongLong(x) => fmt::UpperHex::fmt(x, f),
                UnsignedInt::Isize(x) => fmt::UpperHex::fmt(x, f),
            }
        }
    }

    impl fmt::Octal for UnsignedInt {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                UnsignedInt::Int(x) => fmt::Octal::fmt(x, f),
                UnsignedInt::Char(x) => fmt::Octal::fmt(x, f),
                UnsignedInt::Short(x) => fmt::Octal::fmt(x, f),
                UnsignedInt::Long(x) => fmt::Octal::fmt(x, f),
                UnsignedInt::LongLong(x) => fmt::Octal::fmt(x, f),
                UnsignedInt::Isize(x) => fmt::Octal::fmt(x, f),
            }
        }
    }

    /// An argument as passed to [`format()`].
    #[derive(Debug, Copy, Clone, PartialEq)]
    pub struct Argument<'a> {
        pub flags: Flags,
        pub width: c_int,
        pub precision: Option<c_int>,
        pub specifier: Specifier<'a>,
    }

    impl<'a> From<Specifier<'a>> for Argument<'a> {
        fn from(specifier: Specifier<'a>) -> Self {
            Self {
                flags: Flags::empty(),
                width: 0,
                precision: None,
                specifier,
            }
        }
    }

    /// A [format specifier](https://en.wikipedia.org/wiki/Printf_format_string#Type_field).
    #[derive(Debug, Copy, Clone, PartialEq)]
    #[non_exhaustive]
    pub enum Specifier<'a> {
        /// `%`
        Percent,
        /// `d`, `i`
        Int(SignedInt),
        /// `u`
        Uint(UnsignedInt),
        /// `o`
        Octal(UnsignedInt),
        /// `f`, `F`, `e`, `E`, `g`, `G`, `a`, `A`
        Double { value: f64, format: DoubleFormat },
        /// string outside of formatting
        Bytes(&'a [u8]),
        /// `s`
        ///
        /// The same as [`Bytes`][Specifier::Bytes] but guaranteed to be
        /// null-terminated. This can be used for optimizations, where if you
        /// need to null terminate a string to print it, you can skip that step.
        String(&'a CStr),
        /// `c`
        Char(c_char),
        /// `x`
        Hex(UnsignedInt),
        /// `X`
        UpperHex(UnsignedInt),
        /// `p`
        Pointer(*const ()),
        /// `n`
        ///
        /// # Safety
        ///
        /// This can be a serious security vulnerability if the format specifier
        /// of `printf` is allowed to be user-specified. This shouldn't ever
        /// happen, but poorly-written software may do so.
        WriteBytesWritten(c_int, *const c_int),
    }
}

#[cfg(test)]
mod tests {
    use super::{format, output};
    use crate::va::{vararg_entry, VaListTag};
    use alloc::string::String;
    use libc::{c_char, c_int};

    /// The implementation half of the test entry: run the REAL formatting
    /// path (tag walk, parser, formatter) into a byte buffer, NUL
    /// terminate it, and hand back `format`'s return value.
    unsafe extern "C" fn fmt_into_impl(
        out: *mut u8,
        cap: usize,
        fmt: *const c_char,
        tag: *mut VaListTag,
    ) -> c_int {
        // SAFETY: the entry hands us a live tag, and every test call site
        // passes arguments matching its format string and a buffer with
        // room for the formatted bytes plus a NUL.
        unsafe {
            let mut s = String::new();
            let n = format(fmt, &mut *tag, output::fmt_write(&mut s));
            if n < 0 {
                return n;
            }
            let bytes = s.as_bytes();
            assert!(bytes.len() < cap, "test buffer too small");
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
            *out.add(bytes.len()) = 0;
            n
        }
    }

    vararg_entry! {
        #[no_mangle]
        unsafe extern "C" fn __taproot_printf_test_fmt(
            out: *mut u8,
            cap: usize,
            fmt: *const c_char,
            ...
        ) -> c_int => fmt_into_impl
    }

    /// The C-side view of the entry, as in va.rs's suite: calling through
    /// this makes each test a real-ABI round trip, with rustc performing
    /// the caller half of the variadic protocol.
    mod decl {
        use libc::{c_char, c_int};

        unsafe extern "C" {
            pub fn __taproot_printf_test_fmt(
                out: *mut u8,
                cap: usize,
                fmt: *const c_char,
                ...
            ) -> c_int;
        }
    }

    fn formatted(buf: &[u8], n: c_int) -> &[u8] {
        assert!(n >= 0, "formatting failed: {n}");
        let n = n as usize;
        assert_eq!(buf[n], 0, "missing NUL terminator");
        &buf[..n]
    }

    #[test]
    fn formats_d_s_f_exactly() {
        let mut buf = [0xAA_u8; 64];
        let n = unsafe {
            decl::__taproot_printf_test_fmt(
                buf.as_mut_ptr(),
                buf.len(),
                c"%d %s %.2f".as_ptr(),
                42 as c_int,
                c"carrot".as_ptr(),
                3.14159_f64,
            )
        };
        assert_eq!(formatted(&buf, n), b"42 carrot 3.14");
        assert_eq!(n, 14);
    }

    #[test]
    fn width_precision_and_alignment() {
        // A zero-padded width on a precision'd double, a left-aligned
        // width on an int, and a right-aligned width on a string.
        let mut buf = [0xAA_u8; 64];
        let n = unsafe {
            decl::__taproot_printf_test_fmt(
                buf.as_mut_ptr(),
                buf.len(),
                c"%08.3f|%-6d|%5s".as_ptr(),
                2.5_f64,
                42 as c_int,
                c"ab".as_ptr(),
            )
        };
        assert_eq!(formatted(&buf, n), b"0002.500|42    |   ab");
    }

    #[test]
    fn star_width_comes_off_the_walk() {
        // `%*d` fetches its width as an argument before the value.
        let mut buf = [0xAA_u8; 32];
        let n = unsafe {
            decl::__taproot_printf_test_fmt(
                buf.as_mut_ptr(),
                buf.len(),
                c"%*d".as_ptr(),
                5 as c_int,
                42 as c_int,
            )
        };
        assert_eq!(formatted(&buf, n), b"   42");
    }

    #[test]
    fn args_cross_the_overflow_area() {
        // Three named INTEGER arguments plus seven variadic ints exhaust
        // the six GP registers mid-walk, and the double rides xmm0: the
        // formatter's fetches must agree with the walker across both the
        // register and stack areas.
        let mut buf = [0xAA_u8; 64];
        let n = unsafe {
            decl::__taproot_printf_test_fmt(
                buf.as_mut_ptr(),
                buf.len(),
                c"%d %d %d %d %d %d %d %.1f".as_ptr(),
                1 as c_int,
                2 as c_int,
                3 as c_int,
                4 as c_int,
                5 as c_int,
                6 as c_int,
                7 as c_int,
                8.5_f64,
            )
        };
        assert_eq!(formatted(&buf, n), b"1 2 3 4 5 6 7 8.5");
    }

    #[test]
    fn percent_n_is_rejected() {
        // The vendored formatter parses `%n` but refuses to write through
        // it, reporting an error instead; the guard c-scape's `__*_chk`
        // comments rely on.
        let mut sink: c_int = 0;
        let mut buf = [0xAA_u8; 32];
        let n = unsafe {
            decl::__taproot_printf_test_fmt(
                buf.as_mut_ptr(),
                buf.len(),
                c"a%nb".as_ptr(),
                &mut sink as *mut c_int,
            )
        };
        assert_eq!(n, -1);
        assert_eq!(sink, 0);
    }
}
