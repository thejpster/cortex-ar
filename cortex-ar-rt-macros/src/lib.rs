//! Macros for the cortex-a-rt and cortex-r-rt libraries
//!
//! Provides `#[entry]` and `#[exception(UndefinedHandler)]` attribute macros.
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

    if let Err(error) = check_attr_whitelist(&f.attrs, WhiteListCaller::Entry) {
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
    UndefinedHandler,
    SvcHandler,
    PrefetchHandler,
    AbortHandler,
    IrqHandler,
}

impl std::fmt::Display for Exception {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Exception::UndefinedHandler => write!(f, "UndefinedHandler"),
            Exception::SvcHandler => write!(f, "SvcHandler"),
            Exception::PrefetchHandler => write!(f, "PrefetchHandler"),
            Exception::AbortHandler => write!(f, "AbortHandler"),
            Exception::IrqHandler => write!(f, "IrqHandler"),
        }
    }
}

/// Creates an `unsafe` exception handler.
///
/// When placed on a function like:
///
/// ```rust ignore
/// #[exception(UndefinedHandler)]
/// fn foo(addr: usize) -> ! {
///     panic!("On no")
/// }
/// ```
///
/// You get something like:
///
/// ```rust
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
/// * UndefinedHandler (creates `_undefined_handler`)
/// * SvcHandler (creates `_svc_handler`)
/// * PrefetchHandler (creates `_prefetch_handler`)
/// * AbortHandler (creates `_abort_handler`)
/// * IrqHandler (creates `_irq_handler`)
#[proc_macro_attribute]
pub fn exception(args: TokenStream, input: TokenStream) -> TokenStream {
    let f = parse_macro_input!(input as ItemFn);

    if let Err(error) = check_attr_whitelist(&f.attrs, WhiteListCaller::Exception) {
        return error;
    }

    let mut args_iter = args.into_iter();
    let Some(TokenTree::Ident(exception_name)) = args_iter.next() else {
        return parse::Error::new(
            Span::call_site(),
            "This attribute requires the name of the exception as the first argument",
        )
        .to_compile_error()
        .into();
    };
    if !args_iter.next().is_none() {
        return parse::Error::new(
            Span::call_site(),
            "This attribute accepts only one argument",
        )
        .to_compile_error()
        .into();
    }

    let exception_name = exception_name.to_string();

    let exn = match exception_name.as_str() {
        "UndefinedHandler" => Exception::UndefinedHandler,
        "SvcHandler" => Exception::SvcHandler,
        "PrefetchHandler" => Exception::PrefetchHandler,
        "AbortHandler" => Exception::AbortHandler,
        "IrqHandler" => Exception::IrqHandler,
        _ => {
            return parse::Error::new(f.sig.ident.span(), "This is not a valid exception name")
                .to_compile_error()
                .into();
        }
    };

    let returns_never = match f.sig.output {
        ReturnType::Type(_, ref ty) => matches!(**ty, Type::Never(_)),
        _ => false,
    };
    let ident = &f.sig.ident;
    let (ref cfgs, ref attrs) = extract_cfgs(f.attrs.clone());

    let handler = match exn {
        // extern "C" fn _undefined_handler(addr: usize) -> !;
        // extern "C" fn _undefined_handler(addr: usize) -> usize;
        Exception::UndefinedHandler => {
            let tramp_ident = Ident::new("__cortex_ar_rt_undefined_handler", Span::call_site());
            if returns_never {
                quote!(
                    #(#cfgs)*
                    #(#attrs)*
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
                    #[export_name = "_undefined_handler"]
                    pub unsafe extern "C" fn #tramp_ident(addr: usize) -> usize {
                        #ident(addr)
                    }

                    #[allow(non_snake_case)]
                    #f
                )
            }
        }
        // extern "C" fn _prefetch_handler(addr: usize) -> !;
        // extern "C" fn _prefetch_handler(addr: usize) -> usize;
        Exception::PrefetchHandler => {
            let tramp_ident = Ident::new("__cortex_ar_rt_prefetch_handler", Span::call_site());
            if returns_never {
                quote!(
                    #(#cfgs)*
                    #(#attrs)*
                    #[export_name = "_prefetch_handler"]
                    pub unsafe extern "C" fn #tramp_ident(addr: usize) -> ! {
                        #ident(addr)
                    }

                    #f
                )
            } else {
                quote!(
                    #(#cfgs)*
                    #(#attrs)*
                    #[export_name = "_prefetch_handler"]
                    pub unsafe extern "C" fn #tramp_ident(addr: usize) -> usize {
                        #ident(addr)
                    }

                    #[allow(non_snake_case)]
                    #f
                )
            }
        }
        // extern "C" fn _abort_handler(addr: usize) -> !;
        // extern "C" fn _abort_handler(addr: usize) -> usize;
        Exception::AbortHandler => {
            let tramp_ident = Ident::new("__cortex_ar_rt_abort_handler", Span::call_site());
            if returns_never {
                quote!(
                    #(#cfgs)*
                    #(#attrs)*
                    #[export_name = "_abort_handler"]
                    pub unsafe extern "C" fn #tramp_ident(addr: usize) -> ! {
                        #ident(addr)
                    }

                    #f
                )
            } else {
                quote!(
                    #(#cfgs)*
                    #(#attrs)*
                    #[export_name = "_abort_handler"]
                    pub unsafe extern "C" fn #tramp_ident(addr: usize) -> usize {
                        #ident(addr)
                    }

                    #[allow(non_snake_case)]
                    #f
                )
            }
        }
        // extern "C" fn _svc_handler(addr: usize);
        Exception::SvcHandler => {
            let tramp_ident = Ident::new("__cortex_ar_rt_svc_handler", Span::call_site());
            quote!(
                #(#cfgs)*
                #(#attrs)*
                #[export_name = "_svc_handler"]
                pub unsafe extern "C" fn #tramp_ident(arg: u32) {
                    #ident(arg)
                }

                #[allow(non_snake_case)]
                #f
            )
        }
        // extern "C" fn _irq_handler(addr: usize);
        Exception::IrqHandler => {
            let tramp_ident = Ident::new("__cortex_ar_rt_irq_handler", Span::call_site());
            quote!(
                #(#cfgs)*
                #(#attrs)*
                #[export_name = "_irq_handler"]
                pub unsafe extern "C" fn #tramp_ident() {
                    #ident()
                }

                #[allow(non_snake_case)]
                #f
            )
        }
    };

    quote!(
        #handler
    )
    .into()
}

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

enum WhiteListCaller {
    Entry,
    Exception,
}

fn check_attr_whitelist(attrs: &[Attribute], caller: WhiteListCaller) -> Result<(), TokenStream> {
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
            WhiteListCaller::Entry => {
                "this attribute is not allowed on a cortex-r-rt/cortex-a-rt entry point"
            }
            WhiteListCaller::Exception => {
                "this attribute is not allowed on an exception handler controlled by cortex-r-rt/cortex-a-rt"
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
