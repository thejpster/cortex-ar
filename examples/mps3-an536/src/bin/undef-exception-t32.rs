//! Example triggering a undef exception.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, Ordering};
use semihosting::println;

// pull in our start-up code
use mps3_an536 as _;

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// The entry-point to the Rust application.
///
/// It is called by the start-up.
#[no_mangle]
pub extern "C" fn kmain() -> ! {
    println!("Hello, this is a undef exception example");

    unsafe {
        // trigger an Undefined exception, from T32 (Thumb) mode
        udf_from_t32();
    }

    // this should be impossible because returning from the fault handler will
    // immediately trigger the fault again.

    unreachable!("should never be here!");
}

// These functions are written in assembly
extern "C" {
    fn udf_from_t32();
}

core::arch::global_asm!(
    r#"
    // fn udf_from_t32();
    .thumb
    .global udf_from_t32
    .type udf_from_t32, %function
    udf_from_t32:
        udf     #0
        bx      lr
    .size udf_from_t32, . - udf_from_t32
"#
);

#[unsafe(no_mangle)]
unsafe extern "C" fn _prefetch_handler(_addr: usize) -> ! {
    panic!("unexpected undefined exception");
}

#[unsafe(no_mangle)]
unsafe extern "C" fn _undefined_handler(addr: usize) -> usize {
    println!("undefined abort occurred");

    if (addr + 1) == udf_from_t32 as usize {
        // note that thumb functions have their LSB set, despite always being a
        // multiple of two - that's how the CPU knows they are written in T32
        // machine code.
        println!("caught udf_from_t32");
    } else {
        println!(
            "Bad fault address {:08x} is not {:08x}",
            addr, udf_from_t32 as usize
        );
    }

    if COUNTER.fetch_add(1, Ordering::Relaxed) == 1 {
        // we've faulted twice - time to quit
        semihosting::process::exit(0);
    }

    addr
}

#[unsafe(no_mangle)]
unsafe extern "C" fn _abort_handler(_addr: usize) -> ! {
    panic!("unexpected abort exception");
}
