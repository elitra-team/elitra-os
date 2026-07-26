/// System-wide tunable constants for Elitra OS.
/// Modify these values to adjust OS limits and behavior.

/// Maximum number of concurrent tasks (processes/threads)
pub const MAX_TASKS: u32 = 128;

/// Maximum file descriptors per process
pub const MAX_FDS: usize = 128;

/// Maximum number of pipes in the system
pub const MAX_PIPES: usize = 64;

/// Maximum number of network sockets
pub const MAX_SOCKETS: usize = 32;

/// Maximum number of sleep queue entries
pub const SLEEP_QUEUE_MAX: usize = 128;

/// Maximum number of CPUs supported
pub const MAX_CPUS: usize = 256;

/// User stack size in pages (each page is 4KB)
pub const USTACK_PAGES: u32 = 16;

/// Initial program break address (heap start)
pub const BRK_INITIAL: u64 = 0x10000000;

/// Memory-mapped region start address
pub const MMAP_VADDR: u64 = 0x40000000;

/// User stack virtual address (top)
pub const USTACK_VADDR: u64 = 0xC0000000;

/// Page size in bytes
pub const PAGE_SIZE: u64 = 4096;

/// Maximum number of signals
pub const NSIG: usize = 32;

/// Maximum number of VMA (Virtual Memory Area) entries
pub const MAX_VMA_ENTRIES: usize = 64;

/// Maximum number of ELF program headers
pub const MAX_ELF_PHDRS: usize = 16;
