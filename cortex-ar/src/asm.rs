//! Simple assembly routines

use core::sync::atomic::{compiler_fence, Ordering};

/// Data Synchronization Barrier
///
/// Acts as a special kind of memory barrier. No instruction in program order after this instruction
/// can execute until this instruction completes. This instruction completes only when both:
///
///  * any explicit memory access made before this instruction is complete
///  * all cache and branch predictor maintenance operations before this instruction complete
#[inline]
pub fn dsb() {
    compiler_fence(Ordering::SeqCst);
    unsafe {
        core::arch::asm!("dsb", options(nostack, preserves_flags));
    }
    compiler_fence(Ordering::SeqCst);
}

/// Instruction Synchronization Barrier
///
/// Flushes the pipeline in the processor, so that all instructions following the `ISB` are fetched
/// from cache or memory, after the instruction has been completed.
#[inline]
pub fn isb() {
    compiler_fence(Ordering::SeqCst);
    unsafe {
        core::arch::asm!("isb", options(nostack, preserves_flags));
    }
    compiler_fence(Ordering::SeqCst);
}

/// Emit an NOP instruction
#[inline]
pub fn nop() {
    unsafe { core::arch::asm!("nop", options(nomem, nostack, preserves_flags)) }
}

/// Emit an WFI instruction
#[inline]
pub fn wfi() {
    unsafe { core::arch::asm!("wfi", options(nomem, nostack, preserves_flags)) }
}

/// Emit an WFE instruction
#[inline]
pub fn wfe() {
    unsafe { core::arch::asm!("wfe", options(nomem, nostack, preserves_flags)) }
}
