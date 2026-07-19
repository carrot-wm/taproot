//! Limited wrapper around the `#[naked]` attribute.
//!
//! # Example
//!
//! ```no_compile
//! naked_fn!(
//!     "
//!     A documentation comment.
//!
//!     In the macro expansion, this string will be expanded as a documentation
//!     comment for the generated code.
//!     ";
//!
//!     // Declare a `pub(crate` function named `function_name` with no
//!     // arguments that does not return.
//!     pub(crate) fn function_name() -> !;
//!
//!     // Assembly code for the body.
//!     "assembly code here",
//!     "more assembly code here",
//!     "we can even use {symbols} like {this}",
//!
//!     // Provide symbols for use in the assembly code.
//!     symbols = sym path::to::symbols,
//!     this = sym path::to::this
//! );
//! ```

#![allow(unused_macros)]

/// `#[unsafe(naked)]` has been stable since Rust 1.88, so the entry is a
/// real Rust function on every toolchain (a `global_asm!` fallback used to
/// live here; a naked fn is better than asm because rustc knows the
/// symbol, which keeps it in a cdylib's exported-symbol list where the
/// linker's version script would demote an asm-defined global to local).
/// This macro supports a limited subset of the features of `#[naked]`.
macro_rules! naked_fn {
    (
        $doc:literal;
        $vis:vis fn $name:ident $args:tt -> $ret:ty;
        $($code:literal),*;
        $($label:ident = $kind:ident $path:path),*
    ) => {
        #[doc = $doc]
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        $vis unsafe extern "C" fn $name $args -> $ret {
            core::arch::naked_asm!(
                $($code),*,
                $($label = $kind $path),*
            )
        }
    };
}
