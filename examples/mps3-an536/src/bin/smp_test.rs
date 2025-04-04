//! Multi-core hello-world for Arm Cortex-R
//!
//! Runs code on two cores, checking that atomic fetch_add works.
//!
//! Abuses the FPGA LED register as a place to record whether Core 0 has
//! started.
//!
//! Run with `cargo run --bin smp_test --target=armv8r-none-eabihf -- -smp 2`.

#![no_std]
#![no_main]

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU32, Ordering};

// pull in our start-up code
use mps3_an536 as _;

use semihosting::println;

#[repr(align(16))]
struct Stack<const LEN_BYTES: usize> {
    contents: UnsafeCell<[u8; LEN_BYTES]>,
}

impl<const LEN_BYTES: usize> Stack<LEN_BYTES> {
    const fn new() -> Self {
        Self {
            contents: UnsafeCell::new([0u8; LEN_BYTES]),
        }
    }

    fn stack_top(&self) -> usize {
        let stack_start = self.contents.get() as usize;
        stack_start + LEN_BYTES
    }
}

unsafe impl<const LEN_BYTES: usize> Sync for Stack<LEN_BYTES> {}

static CORE1_STACK: Stack<65536> = Stack::new();

static SHARED_VARIABLE: AtomicU32 = AtomicU32::new(0);

/// The entry-point to the Rust application.
///
/// It is called by the start-up code in `cortex-m-rt`.
#[no_mangle]
pub extern "C" fn kmain() {
    let fpga_led = 0xE020_2000 as *mut u32;
    extern "C" {
        static mut _core1_stack_pointer: usize;
    }
    unsafe {
        let p = &raw mut _core1_stack_pointer;
        p.write(CORE1_STACK.stack_top());
    }
    unsafe {
        // Activate second core by writing to FPGA LEDs.
        // We needed a shared register that wasn't in RAM, and this will do.
        fpga_led.write_volatile(1);
    }

    // wait some time for core 1 to start
    for counter in 0..1000 {
        if SHARED_VARIABLE.load(Ordering::SeqCst) != 0 {
            break;
        }
        if counter == 999 {
            println!("CPU 1 is missing?!");

            semihosting::process::exit(0);
        }
    }

    for _ in 0..1000 {
        SHARED_VARIABLE.fetch_add(1, Ordering::Relaxed);
    }

    println!(
        "Total is {} (is it 2001?)",
        SHARED_VARIABLE.load(Ordering::Relaxed)
    );

    semihosting::process::exit(0);
}

/// The entry-point to the Rust application.
///
/// It is called by the start-up code below, on Core 1.
#[no_mangle]
pub extern "C" fn kmain2() {
    SHARED_VARIABLE.store(1, Ordering::SeqCst);
    for _ in 0..1000 {
        SHARED_VARIABLE.fetch_add(1, Ordering::Relaxed);
    }
    loop {
        core::hint::spin_loop();
    }
}

// Start-up code for multi-core Armv8-R, as implemented on the MPS3-AN536.
//
// We boot into EL2, set up a stack pointer, init .data on .bss on core0, and
// run `kmain` in EL1 on all cores.
#[cfg(arm_architecture = "v8-r")]
core::arch::global_asm!(
    r#"
    .section .bss
    .align 4
    _core1_stack_pointer:
        .word 0

    .section .text.startup
    .align 4

    .global _start
    .global core1_released
    .type _start, %function
    _start:
        // Read MPIDR into R0
        mrc     p15, 0, r0, c0, c0, 5
        ands    r0, r0, 0xFF
        bne     core1
    core0:
        ldr     pc, =_default_start
    core1:
        ldr     r0, =0xE0202000
        mov     r1, #0
    core1_spin:
        wfe
        // spin until an LED0 is on
        ldr     r2, [r0]  
        cmp     r1, r2
        beq     core1_spin
    core1_released:
        // now an LED is on, we assume _core1_stack_pointer contains our stack pointer
        // First we must exit EL2...
        // Set the HVBAR (for EL2) to _vector_table
        ldr     r0, =_vector_table
        mcr     p15, 4, r0, c12, c0, 0
        // Configure HACTLR to let us enter EL1
        mrc     p15, 4, r0, c1, c0, 1
        mov     r1, {hactlr_bits}
        orr     r0, r0, r1
        mcr     p15, 4, r0, c1, c0, 1
        // Program the SPSR - enter system mode (0x1F) in Arm mode with IRQ, FIQ masked
        mov		r0, {sys_mode}
        msr		spsr_hyp, r0
        adr		r0, 1f
        msr		elr_hyp, r0
        dsb
        isb
        eret
    1:
        // Set the VBAR (for EL1) to _vector_table. NB: This isn't required on
        // Armv7-R because that only supports 'low' (default) or 'high'.
        ldr     r0, =_vector_table
        mcr     p15, 0, r0, c12, c0, 0
        ldr     r0, =_core1_stack_pointer
        ldr     r0, [r0]
        // set up our stacks using that stack pointer
        bl      _stack_setup
        bl      kmain2
    .size _start, . - _start
    "#,
    hactlr_bits = const {
        cortex_ar::register::Hactlr::new_with_raw_value(0)
            .with_cpuactlr(true)
            .with_cdbgdci(true)
            .with_flashifregionr(true)
            .with_periphpregionr(true)
            .with_qosr(true)
            .with_bustimeoutr(true)
            .with_intmonr(true)
            .with_err(true)
            .with_testr1(true)
            .raw_value()
    },
    sys_mode = const {
        cortex_ar::register::Cpsr::new_with_raw_value(0)
            .with_mode(cortex_ar::register::cpsr::ProcessorMode::Sys)
            .with_i(true)
            .with_f(true)
            .raw_value()
    }
);
