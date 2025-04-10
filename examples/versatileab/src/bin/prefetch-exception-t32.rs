//! Example triggering a prefetch exception.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, Ordering};
use cortex_ar::register::{Ifar, Ifsr};
use semihosting::println;

// pull in our start-up code
use versatileab as _;

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// The entry-point to the Rust application.
///
/// It is called by the start-up.
#[no_mangle]
pub extern "C" fn kmain() -> ! {
    println!("Hello, this is a prefetch exception example");

    // A BKPT instruction triggers a Prefetch Abort except when Halting debug-mode is enabled.
    // See p. 2038 of ARMv7-M Architecture Reference Manual
    unsafe {
        // trigger an prefetch exception, from T32 (Thumb) mode
        bkpt_from_t32();
    }

    // this should be impossible because returning from the fault handler will
    // immediately trigger the fault again.

    unreachable!("should never be here!");
}

// These functions are written in assembly
extern "C" {
    fn bkpt_from_t32();
}

core::arch::global_asm!(
    r#"
    // fn bkpt_from_t32();
    .thumb
    .global bkpt_from_t32
    .type bkpt_from_t32, %function
    bkpt_from_t32:
        bkpt    #0
        bx      lr
    .size bkpt_from_t32, . - bkpt_from_t32
"#
);

#[unsafe(no_mangle)]
unsafe extern "C" fn _undefined_handler(_addr: usize) -> ! {
    panic!("unexpected undefined exception");
}

#[unsafe(no_mangle)]
unsafe extern "C" fn _prefetch_handler(addr: usize) -> usize {
    println!("prefetch abort occurred");
    let ifsr = Ifsr::read();
    println!("IFSR (Fault Status Register): {:?}", ifsr);
    println!("IFSR Status: {:?}", ifsr.status());
    let ifar = Ifar::read();
    println!("IFAR (Faulting Address Register): {:?}", ifar);

    if (addr + 1) == bkpt_from_t32 as usize {
        // note that thumb functions have their LSB set, despite always being a
        // multiple of two - that's how the CPU knows they are written in T32
        // machine code.
        println!("caught bkpt_from_t32");
    } else {
        println!(
            "Bad fault address {:08x} is not {:08x}",
            addr, bkpt_from_t32 as usize
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
