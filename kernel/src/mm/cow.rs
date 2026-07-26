use core::sync::atomic::{AtomicPtr, AtomicU16, Ordering};
use core::ptr;

static REFCOUNT: AtomicPtr<AtomicU16> = AtomicPtr::new(ptr::null_mut());
static TOTAL_FRAMES: AtomicUsize = AtomicUsize::new(0);

use core::sync::atomic::AtomicUsize;

#[no_mangle]
pub unsafe extern "C" fn krust_cow_init(total_frames: usize) -> i32 {
    let size = total_frames * core::mem::size_of::<AtomicU16>();
    let frames_needed = (size + 4095) / 4096;
    let first = krust_pmm_alloc_frame();
    if first == !0usize {
        return -1;
    }
    let base = (first * 4096) as *mut AtomicU16;
    REFCOUNT.store(base, Ordering::Relaxed);
    TOTAL_FRAMES.store(total_frames, Ordering::Relaxed);
    for _ in 1..frames_needed {
        let n = krust_pmm_alloc_frame();
        if n == !0usize {
            return -1;
        }
    }
    for i in 0..total_frames {
        ptr::write(base.add(i), AtomicU16::new(0));
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_cow_refinc(frame: usize) {
    if frame >= TOTAL_FRAMES.load(Ordering::Relaxed) {
        return;
    }
    let base = REFCOUNT.load(Ordering::Relaxed);
    if base.is_null() {
        return;
    }
    let entry = &*base.add(frame);
    let prev = entry.fetch_add(1, Ordering::AcqRel);
    if prev >= 0xFFFE {
        entry.store(0xFFFF, Ordering::Relaxed);
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_cow_refdec(frame: usize) -> u32 {
    if frame >= TOTAL_FRAMES.load(Ordering::Relaxed) {
        return 0;
    }
    let base = REFCOUNT.load(Ordering::Relaxed);
    if base.is_null() {
        return 0;
    }
    let entry = &*base.add(frame);
    let prev = entry.load(Ordering::Acquire);
    if prev > 0 {
        entry.fetch_sub(1, Ordering::AcqRel);
    }
    prev as u32
}

#[no_mangle]
pub unsafe extern "C" fn krust_cow_refcount(frame: usize) -> u32 {
    if frame >= TOTAL_FRAMES.load(Ordering::Relaxed) {
        return 0;
    }
    let base = REFCOUNT.load(Ordering::Relaxed);
    if base.is_null() {
        return 0;
    }
    (*base.add(frame)).load(Ordering::Relaxed) as u32
}

extern "C" {
    fn krust_pmm_alloc_frame() -> usize;
}
