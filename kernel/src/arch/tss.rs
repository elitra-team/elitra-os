
const MAX_CPUS_TSS: usize = 256;

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct TSSEntry64 {
    pub reserved0: u32,
    pub rsp0: u64,
    pub rsp1: u64,
    pub rsp2: u64,
    pub reserved1: u64,
    pub ist1: u64,
    pub ist2: u64,
    pub ist3: u64,
    pub ist4: u64,
    pub ist5: u64,
    pub ist6: u64,
    pub ist7: u64,
    pub reserved2: u64,
    pub reserved3: u16,
    pub iomap_base: u16,
}

const fn empty_tss() -> TSSEntry64 {
    TSSEntry64 {
        reserved0: 0,
        rsp0: 0,
        rsp1: 0,
        rsp2: 0,
        reserved1: 0,
        ist1: 0,
        ist2: 0,
        ist3: 0,
        ist4: 0,
        ist5: 0,
        ist6: 0,
        ist7: 0,
        reserved2: 0,
        reserved3: 0,
        iomap_base: 0,
    }
}

static mut TSS_ENTRIES: [TSSEntry64; MAX_CPUS_TSS] = [empty_tss(); MAX_CPUS_TSS];
static mut TSS_USED: [bool; MAX_CPUS_TSS] = [false; MAX_CPUS_TSS];

pub fn init() {
    unsafe {
        let entry = &mut TSS_ENTRIES[0];
        entry.rsp0 = 0;
        entry.iomap_base = core::mem::size_of::<TSSEntry64>() as u16;
        TSS_USED[0] = true;

        install_tss_for_cpu(0, entry);

        core::arch::asm!("ltr ax", in("ax") 0x28u16);

        crate::vga::krust_vga_writestring_color(
            b"TSS installed (BSP)\n\0" as *const u8,
            0x0A,
        );
        crate::ns16550::krust_ns16550_write_str(b"tss: installed (BSP)\n\0" as *const u8);
    }
}

unsafe fn install_tss_for_cpu(cpu_id: usize, entry: &TSSEntry64) {
    let base = entry as *const _ as u64;
    let limit = core::mem::size_of::<TSSEntry64>() as u32 - 1;

    let gdt_entries = crate::gdt::krust_gdt_entries();
    let desc = gdt_entries.add(5) as *mut u64;
    *desc = (limit & 0xFFFF) as u64
        | ((base & 0xFFFFFF) << 16)
        | (0x89u64 << 40)
        | ((((limit >> 16) & 0x0F) as u64) << 48)
        | ((base & 0xFF000000u64) << 32);
    *desc.add(1) = base >> 32;

    let _ = cpu_id;
}

/// Initialize a TSS for an AP (Application Processor).
/// Returns the TSS entry index on success, or None if no slot is available.
pub unsafe fn init_ap_tss(cpu_id: usize) -> Option<usize> {
    for i in 1..MAX_CPUS_TSS {
        if !TSS_USED[i] {
            TSS_USED[i] = true;
            let entry = &mut TSS_ENTRIES[i];
            entry.rsp0 = 0;
            entry.iomap_base = core::mem::size_of::<TSSEntry64>() as u16;

            crate::ns16550::krust_ns16550_write_str(b"tss: AP slot allocated\n\0" as *const u8);

            return Some(i);
        }
    }
    None
}

/// Load the TSS for the current CPU (uses LTR).
pub unsafe fn load_current_tss(tss_index: usize) {
    if tss_index < MAX_CPUS_TSS {
        let selector = 0x28u16;
        core::arch::asm!("ltr ax", in("ax") selector);
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_tss_set_kernel_stack(rsp: u64) {
    let cpu_id = crate::smp::krust_smp_current_cpu_id() as usize;
    if cpu_id < MAX_CPUS_TSS {
        TSS_ENTRIES[cpu_id].rsp0 = rsp;
    } else {
        TSS_ENTRIES[0].rsp0 = rsp;
    }
}

/// Get a pointer to the TSS entry for a given CPU.
pub unsafe fn tss_entry_for_cpu(cpu_id: usize) -> *mut TSSEntry64 {
    if cpu_id < MAX_CPUS_TSS && TSS_USED[cpu_id] {
        &mut TSS_ENTRIES[cpu_id] as *mut TSSEntry64
    } else {
        core::ptr::null_mut()
    }
}
