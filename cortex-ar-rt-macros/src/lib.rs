//! Macros for the cortex-a-rt and cortex-r-rt libraries
//!
//! Provides `#[entry]`, `#[exception(...)]` and `#[irq]` attribute macros.
//!
//! Do not use this crate directly.
//!
//! Based on <https://github.com/rust-embedded/cortex-m/tree/c-m-rt-v0.7.5/cortex-m-rt/macros>.

extern crate proc_macro;

use proc_macro::{TokenStream, TokenTree};
use proc_macro2::Span;
use quote::quote;
use syn::{
    parse, parse_macro_input, spanned::Spanned, AttrStyle, Attribute, Ident, ItemFn, ReturnType,
    Type, Visibility,
};

/// Creates an `unsafe` program entry point (i.e. a `kmain` function).
///
/// It's `unsafe` because you are not supposed to call it - it should only be
/// called from the start-up code once initialisation is complete.
///
/// When placed on a function like:
///
/// ```rust ignore
/// #[entry]
/// fn foo() -> ! {
///     panic!("On no")
/// }
/// ```
///
/// You get something like:
///
/// ```rust
/// #[doc(hidden)]
/// #[export_name = "kmain"]
/// pub unsafe extern "C" fn __cortex_ar_rt_kmain() -> ! {
///     foo()
/// }
///
/// fn foo() -> ! {
///     panic!("On no")
/// }
/// ```
///
/// The symbol `kmain` is what the assembly code in both the cortex-r-rt and
/// cortex-a-rt start-up code will jump to, and the `extern "C"` makes it sound
/// to call from assembly.
#[proc_macro_attribute]
pub fn entry(args: TokenStream, input: TokenStream) -> TokenStream {
    let f = parse_macro_input!(input as ItemFn);

    // check the function signature.
    //
    // it should be `fn foo() -> !` or `unsafe fn foo() -> !`
    let valid_signature = f.sig.constness.is_none()
        && f.vis == Visibility::Inherited
        && f.sig.abi.is_none()
        && f.sig.inputs.is_empty()
        && f.sig.generics.params.is_empty()
        && f.sig.generics.where_clause.is_none()
        && f.sig.variadic.is_none()
        && match f.sig.output {
            ReturnType::Default => false,
            ReturnType::Type(_, ref ty) => matches!(**ty, Type::Never(_)),
        };

    if !valid_signature {
        return parse::Error::new(
            f.span(),
            "`#[entry]` function must have signature `[unsafe] fn() -> !`",
        )
        .to_compile_error()
        .into();
    }

    if !args.is_empty() {
        return parse::Error::new(Span::call_site(), "This attribute accepts no arguments")
            .to_compile_error()
            .into();
    }

    let tramp_ident = Ident::new("__cortex_ar_rt_kmain", Span::call_site());
    let ident = &f.sig.ident;

    if let Err(error) = check_attr_whitelist(&f.attrs, Kind::Entry) {
        return error;
    }

    let (ref cfgs, ref attrs) = extract_cfgs(f.attrs.clone());

    quote!(
        #(#cfgs)*
        #(#attrs)*
        #[doc(hidden)]
        #[export_name = "kmain"]
        pub unsafe extern "C" fn #tramp_ident() -> ! {
            #ident()
        }

        #f
    )
    .into()
}

/// The set of exceptions we can handle.
#[derive(Debug, PartialEq)]
enum Exception {
    Undefined,
    SupervisorCall,
    PrefetchAbort,
    DataAbort,
    Irq,
}

impl std::fmt::Display for Exception {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Exception::Undefined => write!(f, "Undefined"),
            Exception::SupervisorCall => write!(f, "SupervisorCall"),
            Exception::PrefetchAbort => write!(f, "PrefetchAbort"),
            Exception::DataAbort => write!(f, "DataAbort"),
            Exception::Irq => write!(f, "Irq"),
        }
    }
}

/// Creates an `unsafe` exception handler.
///
/// It's `unsafe` because you are not supposed to call it - it should only be
/// called from assembly routines registered in the interrupt vector table.
///
/// When placed on a function like:
///
/// ```rust ignore
/// #[exception(Undefined)]
/// fn foo(addr: usize) -> ! {
///     panic!("On no")
/// }
/// ```
///
/// You get something like:
///
/// ```rust
/// #[doc(hidden)]
/// #[export_name = "_undefined_handler"]
/// pub unsafe extern "C" fn __cortex_ar_rt_undefined_handler(addr: usize) -> ! {
///     foo(addr)
/// }
///
/// fn foo(addr: usize) -> ! {
///     panic!("On no")
/// }
/// ```
///
/// The supported arguments are:
///
/// * Undefined (creates `_undefined_handler`)
/// * SupervisorCall (creates `_svc_handler`)
/// * PrefetchAbort (creates `_prefetch_abort_handler`)
/// * DataAbort (creates `_data_abort_handler`)
/// * Irq (creates `_irq_handler`) - although people should prefer `#[irq]`.
#[proc_macro_attribute]
pub fn exception(args: TokenStream, input: TokenStream) -> TokenStream {
    handle_exception_interrupt(args, input, Kind::Exception)
}

/// Creates an `unsafe` interrupt handler.
///
/// It's `unsafe` because you are not supposed to call it - it should only be
/// called from assembly routines registered in the interrupt vector table.
///
/// When placed on a function like:
///
/// ```rust ignore
/// #[irq]
/// fn foo(addr: usize) -> ! {
///     panic!("On no")
/// }
/// ```
///
/// You get something like:
///
/// ```rust
/// #[doc(hidden)]
/// #[export_name = "_irq_handler"]
/// pub unsafe extern "C" fn __cortex_ar_rt_irq_handler(addr: usize) -> ! {
///     foo(addr)
/// }
///
/// fn foo(addr: usize) -> ! {
///     panic!("On no")
/// }
/// ```
///
/// This is preferred over `#[exception(Irq)` because most people
/// probably won't consider interrupts to be a form of exception.
#[proc_macro_attribute]
pub fn irq(args: TokenStream, input: TokenStream) -> TokenStream {
    handle_exception_interrupt(args, input, Kind::Interrupt)
}

/// Note if we got `#[entry]`, `#[exception(...)]` or `#[irq]`
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum Kind {
    /// Corresponds to `#[entry]`
    Entry,
    /// Corresponds to `#[exception(...)]`
    Exception,
    /// Corresponds to `#[irq]`
    Interrupt,
}

/// A common routine for handling exception or interrupt functions
fn handle_exception_interrupt(args: TokenStream, input: TokenStream, kind: Kind) -> TokenStream {
    let f = parse_macro_input!(input as ItemFn);

    if let Err(error) = check_attr_whitelist(&f.attrs, kind) {
        return error;
    }

    let returns_never = match f.sig.output {
        ReturnType::Type(_, ref ty) => matches!(**ty, Type::Never(_)),
        _ => false,
    };

    let exception = match kind {
        Kind::Entry => {
            panic!("Don't handle #[entry] with `handle_exception_interrupt`!");
        }
        Kind::Exception => {
            let mut args_iter = args.into_iter();
            let Some(TokenTree::Ident(exception_name)) = args_iter.next() else {
                return parse::Error::new(
                    Span::call_site(),
                    "This attribute requires the name of the exception as the first argument",
                )
                .to_compile_error()
                .into();
            };
            if args_iter.next().is_some() {
                return parse::Error::new(
                    Span::call_site(),
                    "This attribute accepts only one argument",
                )
                .to_compile_error()
                .into();
            }
            match exception_name.to_string().as_str() {
                "Undefined" => {
                    if !returns_never && f.sig.unsafety.is_none() {
                        return parse::Error::new(
                            exception_name.span().into(),
                            "Undefined handlers that don't return ! must be unsafe",
                        )
                        .to_compile_error()
                        .into();
                    }
                    Exception::Undefined
                }
                "SupervisorCall" => Exception::SupervisorCall,
                "PrefetchAbort" => {
                    if !returns_never && f.sig.unsafety.is_none() {
                        return parse::Error::new(
                            exception_name.span().into(),
                            "PrefetchAbort handlers that don't return ! must be unsafe",
                        )
                        .to_compile_error()
                        .into();
                    }
                    Exception::PrefetchAbort
                }
                "DataAbort" => {
                    if !returns_never && f.sig.unsafety.is_none() {
                        return parse::Error::new(
                            exception_name.span().into(),
                            "DataAbort handlers that don't return ! must be unsafe",
                        )
                        .to_compile_error()
                        .into();
                    }
                    Exception::DataAbort
                }
                "Irq" => Exception::Irq,
                _ => {
                    return parse::Error::new(
                        exception_name.span().into(),
                        "This is not a valid exception name",
                    )
                    .to_compile_error()
                    .into();
                }
            }
        }
        Kind::Interrupt => Exception::Irq,
    };

    let ident = &f.sig.ident;
    let (ref cfgs, ref attrs) = extract_cfgs(f.attrs.clone());

    let handler = match exception {
        // extern "C" fn _undefined_handler(addr: usize) -> !;
        // unsafe extern "C" fn _undefined_handler(addr: usize) -> usize;
        Exception::Undefined => {
            let tramp_ident = Ident::new("__cortex_ar_rt_undefined_handler", Span::call_site());
            if returns_never {
                quote!(
                    #(#cfgs)*
                    #(#attrs)*
                    #[doc(hidden)]
                    #[export_name = "_undefined_handler"]
                    pub unsafe extern "C" fn #tramp_ident(addr: usize) -> ! {
                        #ident(addr)
                    }

                    #f
                )
            } else {
                quote!(
                    #(#cfgs)*
                    #(#attrs)*
                    #[doc(hidden)]
                    #[export_name = "_undefined_handler"]
                    pub unsafe extern "C" fn #tramp_ident(addr: usize) -> usize {
                        unsafe {
                            #ident(addr)
                        }
                    }

                    #f
                )
            }
        }
        // extern "C" fn _prefetch_abort_handler(addr: usize) -> !;
        // unsafe extern "C" fn _prefetch_abort_handler(addr: usize) -> usize;
        Exception::PrefetchAbort => {
            let tramp_ident =
                Ident::new("__cortex_ar_rt_prefetch_abort_handler", Span::call_site());
            if returns_never {
                quote!(
                    #(#cfgs)*
                    #(#attrs)*
                    #[doc(hidden)]
                    #[export_name = "_prefetch_abort_handler"]
                    pub unsafe extern "C" fn #tramp_ident(addr: usize) -> ! {
                        #ident(addr)
                    }

                    #f
                )
            } else {
                quote!(
                    #(#cfgs)*
                    #(#attrs)*
                    #[doc(hidden)]
                    #[export_name = "_prefetch_abort_handler"]
                    pub unsafe extern "C" fn #tramp_ident(addr: usize) -> usize {
                        unsafe {
                            #ident(addr)
                        }
                    }

                    #f
                )
            }
        }
        // extern "C" fn _data_abort_handler(addr: usize) -> !;
        // unsafe extern "C" fn _data_abort_handler(addr: usize) -> usize;
        Exception::DataAbort => {
            let tramp_ident = Ident::new("__cortex_ar_rt_data_abort_handler", Span::call_site());
            if returns_never {
                quote!(
                    #(#cfgs)*
                    #(#attrs)*
                    #[doc(hidden)]
                    #[export_name = "_data_abort_handler"]
                    pub unsafe extern "C" fn #tramp_ident(addr: usize) -> ! {
                        #ident(addr)
                    }

                    #f
                )
            } else {
                quote!(
                    #(#cfgs)*
                    #(#attrs)*
                    #[doc(hidden)]
                    #[export_name = "_data_abort_handler"]
                    pub unsafe extern "C" fn #tramp_ident(addr: usize) -> usize {
                        unsafe {
                            #ident(addr)
                        }
                    }

                    #f
                )
            }
        }
        // extern "C" fn _svc_handler(addr: usize);
        Exception::SupervisorCall => {
            let tramp_ident = Ident::new("__cortex_ar_rt_svc_handler", Span::call_site());
            quote!(
                #(#cfgs)*
                #(#attrs)*
                #[doc(hidden)]
                #[export_name = "_svc_handler"]
                pub unsafe extern "C" fn #tramp_ident(arg: u32) {
                    #ident(arg)
                }

                #f
            )
        }
        // extern "C" fn _irq_handler(addr: usize);
        Exception::Irq => {
            let tramp_ident = Ident::new("__cortex_ar_rt_irq_handler", Span::call_site());
            quote!(
                #(#cfgs)*
                #(#attrs)*
                #[doc(hidden)]
                #[export_name = "_irq_handler"]
                pub unsafe extern "C" fn #tramp_ident() {
                    #ident()
                }

                #f
            )
        }
    };

    quote!(
        #handler
    )
    .into()
}

/// Given a list of attributes, split them into `cfg` and non-`cfg`.
///
/// Returns `(cfgs, non_cfgs)`.
fn extract_cfgs(attrs: Vec<Attribute>) -> (Vec<Attribute>, Vec<Attribute>) {
    let mut cfgs = vec![];
    let mut not_cfgs = vec![];

    for attr in attrs {
        if eq(&attr, "cfg") {
            cfgs.push(attr);
        } else {
            not_cfgs.push(attr);
        }
    }

    (cfgs, not_cfgs)
}

/// Check whether any disallowed attributes have been applied to our entry/exception function.
fn check_attr_whitelist(attrs: &[Attribute], caller: Kind) -> Result<(), TokenStream> {
    let whitelist = &[
        "doc",
        "link_section",
        "cfg",
        "allow",
        "warn",
        "deny",
        "forbid",
        "cold",
        "naked",
        "expect",
    ];

    'o: for attr in attrs {
        for val in whitelist {
            if eq(attr, val) {
                continue 'o;
            }
        }

        let err_str = match caller {
            Kind::Entry => {
                "this attribute is not allowed on a cortex-r-rt/cortex-a-rt entry point"
            }
            Kind::Exception => {
                "this attribute is not allowed on an exception handler controlled by cortex-r-rt/cortex-a-rt"
            }
            Kind::Interrupt => {
                "this attribute is not allowed on an interrupt handler controlled by cortex-r-rt/cortex-a-rt"
            }
        };

        return Err(parse::Error::new(attr.span(), err_str)
            .to_compile_error()
            .into());
    }

    Ok(())
}

/// Returns `true` if `attr.path` matches `name`
fn eq(attr: &Attribute, name: &str) -> bool {
    attr.style == AttrStyle::Outer && attr.path().is_ident(name)
}
