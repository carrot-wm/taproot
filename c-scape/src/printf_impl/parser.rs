//! The `printf` format-string parser.
//!
//! Vendored from printf-compat 0.4.0 (lights0123, MIT OR Apache-2.0); see
//! the module docs in mod.rs for the port's deltas. Every argument fetch
//! goes through [`VaListTag::arg`], the stable `va_arg` walk.

use core::ffi::*;

use super::{Argument, DoubleFormat, Flags, SignedInt, Specifier, UnsignedInt};
use crate::va::VaListTag;

fn next_char(sub: &[u8]) -> &[u8] {
    sub.get(1..).unwrap_or(&[])
}

/// Parse the [Flags field](https://en.wikipedia.org/wiki/Printf_format_string#Flags_field).
fn parse_flags(mut sub: &[u8]) -> (Flags, &[u8]) {
    let mut flags: Flags = Flags::empty();
    while let Some(&ch) = sub.first() {
        flags.insert(match ch {
            b'-' => Flags::LEFT_ALIGN,
            b'+' => Flags::PREPEND_PLUS,
            b' ' => Flags::PREPEND_SPACE,
            b'0' => Flags::PREPEND_ZERO,
            b'\'' => Flags::THOUSANDS_GROUPING,
            b'#' => Flags::ALTERNATE_FORM,
            _ => break,
        });
        sub = next_char(sub)
    }
    (flags, sub)
}

/// Parse the [Width field](https://en.wikipedia.org/wiki/Printf_format_string#Width_field).
unsafe fn parse_width<'a>(mut sub: &'a [u8], args: &mut VaListTag) -> (c_int, &'a [u8]) {
    let mut width: c_int = 0;
    if sub.first() == Some(&b'*') {
        return (unsafe { args.arg() }, next_char(sub));
    }
    while let Some(&ch) = sub.first() {
        match ch {
            // https://rust-malaysia.github.io/code/2020/07/11/faster-integer-parsing.html#the-bytes-solution
            b'0'..=b'9' => width = width * 10 + (ch & 0x0f) as c_int,
            _ => break,
        }
        sub = next_char(sub);
    }
    (width, sub)
}

/// Parse the [Precision field](https://en.wikipedia.org/wiki/Printf_format_string#Precision_field).
unsafe fn parse_precision<'a>(sub: &'a [u8], args: &mut VaListTag) -> (Option<c_int>, &'a [u8]) {
    match sub.first() {
        Some(&b'.') => {
            let (prec, sub) = unsafe { parse_width(next_char(sub), args) };
            (Some(prec), sub)
        }
        _ => (None, sub),
    }
}

#[derive(Debug, Copy, Clone)]
enum Length {
    Int,
    /// `hh`
    Char,
    /// `h`
    Short,
    /// `l`
    Long,
    /// `ll`
    LongLong,
    /// `z`
    Usize,
    /// `t`
    Isize,
}

impl Length {
    unsafe fn parse_signed(self, args: &mut VaListTag) -> SignedInt {
        match self {
            Length::Int => SignedInt::Int(unsafe { args.arg() }),
            Length::Char => SignedInt::Char(unsafe { args.arg::<c_int>() } as c_schar),
            Length::Short => SignedInt::Short(unsafe { args.arg::<c_int>() } as c_short),
            Length::Long => SignedInt::Long(unsafe { args.arg() }),
            Length::LongLong => SignedInt::LongLong(unsafe { args.arg() }),
            // for some reason, these exist as different options, yet produce the same output
            Length::Usize | Length::Isize => SignedInt::Isize(unsafe { args.arg() }),
        }
    }
    unsafe fn parse_unsigned(self, args: &mut VaListTag) -> UnsignedInt {
        match self {
            Length::Int => UnsignedInt::Int(unsafe { args.arg() }),
            Length::Char => UnsignedInt::Char(unsafe { args.arg::<c_uint>() } as c_uchar),
            Length::Short => UnsignedInt::Short(unsafe { args.arg::<c_uint>() } as c_ushort),
            Length::Long => UnsignedInt::Long(unsafe { args.arg() }),
            Length::LongLong => UnsignedInt::LongLong(unsafe { args.arg() }),
            // for some reason, these exist as different options, yet produce the same output
            Length::Usize | Length::Isize => UnsignedInt::Isize(unsafe { args.arg() }),
        }
    }
}

/// Parse the [Length field](https://en.wikipedia.org/wiki/Printf_format_string#Length_field).
fn parse_length(sub: &[u8]) -> (Length, &[u8]) {
    match sub.first().copied() {
        Some(b'h') => match sub.get(1).copied() {
            Some(b'h') => (Length::Char, sub.get(2..).unwrap_or(&[])),
            _ => (Length::Short, next_char(sub)),
        },
        Some(b'l') => match sub.get(1).copied() {
            Some(b'l') => (Length::LongLong, sub.get(2..).unwrap_or(&[])),
            _ => (Length::Long, next_char(sub)),
        },
        Some(b'z') => (Length::Usize, next_char(sub)),
        Some(b't') => (Length::Isize, next_char(sub)),
        _ => (Length::Int, sub),
    }
}

/// Parse a format parameter and write it somewhere.
///
/// # Safety
///
/// The `args` walk is as unsafe as any `va_list`: the arguments the
/// caller passed must match the passed `format`, which must be a valid
/// [`printf` format string](http://www.cplusplus.com/reference/cstdio/printf/).
pub unsafe fn format(
    format: *const c_char,
    args: &mut VaListTag,
    mut handler: impl FnMut(Argument) -> c_int,
) -> c_int {
    let str = unsafe { CStr::from_ptr(format).to_bytes() };
    // Pair each `%`-split chunk with whether another follows it (the
    // upstream crate used itertools' tuple_windows over an Option chain
    // for the same walk).
    let mut iter = str.split(|&c| c == b'%').peekable();
    let mut written = 0;

    macro_rules! err {
        ($ex: expr) => {{
            let res = $ex;
            if res < 0 {
                return -1;
            } else {
                written += res;
            }
        }};
    }
    if let Some(begin) = iter.next() {
        err!(handler(Specifier::Bytes(begin).into()));
    }
    let mut last_was_percent = false;
    while let Some(sub) = iter.next() {
        let has_next = iter.peek().is_some();
        if last_was_percent {
            err!(handler(Specifier::Bytes(sub).into()));
            last_was_percent = false;
            continue;
        }
        let (flags, sub) = parse_flags(sub);
        let (width, sub) = unsafe { parse_width(sub, args) };
        let (precision, sub) = unsafe { parse_precision(sub, args) };
        let (length, sub) = parse_length(sub);
        let ch = sub.first().unwrap_or(if has_next { &b'%' } else { &0 });
        err!(handler(Argument {
            flags,
            width,
            precision,
            specifier: match ch {
                b'%' => {
                    last_was_percent = true;
                    Specifier::Percent
                }
                b'd' | b'i' => Specifier::Int(unsafe { length.parse_signed(args) }),
                b'x' => Specifier::Hex(unsafe { length.parse_unsigned(args) }),
                b'X' => Specifier::UpperHex(unsafe { length.parse_unsigned(args) }),
                b'u' => Specifier::Uint(unsafe { length.parse_unsigned(args) }),
                b'o' => Specifier::Octal(unsafe { length.parse_unsigned(args) }),
                b'f' | b'F' => Specifier::Double {
                    value: unsafe { args.arg() },
                    format: DoubleFormat::Normal.set_upper(ch.is_ascii_uppercase()),
                },
                b'e' | b'E' => Specifier::Double {
                    value: unsafe { args.arg() },
                    format: DoubleFormat::Scientific.set_upper(ch.is_ascii_uppercase()),
                },
                b'g' | b'G' => Specifier::Double {
                    value: unsafe { args.arg() },
                    format: DoubleFormat::Auto.set_upper(ch.is_ascii_uppercase()),
                },
                b'a' | b'A' => Specifier::Double {
                    value: unsafe { args.arg() },
                    format: DoubleFormat::Hex.set_upper(ch.is_ascii_uppercase()),
                },
                b's' => {
                    let arg: *mut c_char = unsafe { args.arg() };
                    // As a common extension supported by glibc, musl, and
                    // others, format a NULL pointer as "(null)".
                    if arg.is_null() {
                        Specifier::Bytes(b"(null)")
                    } else {
                        Specifier::String(unsafe { CStr::from_ptr(arg) })
                    }
                }
                b'c' => {
                    trait CharToInt {
                        type IntType;
                    }

                    impl CharToInt for c_schar {
                        type IntType = c_int;
                    }

                    impl CharToInt for c_uchar {
                        type IntType = c_uint;
                    }

                    Specifier::Char(
                        unsafe { args.arg::<<c_char as CharToInt>::IntType>() } as c_char
                    )
                }
                b'p' => Specifier::Pointer(unsafe { args.arg() }),
                b'n' => Specifier::WriteBytesWritten(written, unsafe { args.arg() }),
                _ => return -1,
            },
        }));
        err!(handler(Specifier::Bytes(next_char(sub)).into()));
    }
    written
}
