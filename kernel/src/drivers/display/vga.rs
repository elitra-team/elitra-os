use core::ptr;

const VGA_BUF: *mut u16 = 0xB8000 as *mut u16;
const WIDTH: usize = 80;
const HEIGHT: usize = 25;

// Bochs/QEMU VBE registers
const VBE_INDEX: u16 = 0x1CE;
const VBE_DATA: u16 = 0x1CF;
const VBE_XRES: u16 = 1;
const VBE_YRES: u16 = 2;
const VBE_BPP: u16 = 3;
const VBE_ENABLE: u16 = 8;
const VBE_BANK: u16 = 9;
const VBE_VIRT_WIDTH: u16 = 4;
const VBE_VIRT_HEIGHT: u16 = 5;

// Current VGA mode
static mut VGA_MODE_FB: *mut u8 = 0 as *mut u8;
static mut VGA_MODE_W: u32 = 0;
static mut VGA_MODE_H: u32 = 0;
static mut VGA_MODE_PITCH: u32 = 0;

unsafe fn vbe_write(reg: u16, val: u16) {
    core::arch::asm!("out dx, ax", in("dx") VBE_INDEX, in("ax") reg);
    core::arch::asm!("out dx, ax", in("dx") VBE_DATA, in("ax") val);
}

unsafe fn vbe_read(reg: u16) -> u16 {
    let val: u16;
    core::arch::asm!("out dx, ax", in("dx") VBE_INDEX, in("ax") reg);
    core::arch::asm!("in ax, dx", out("ax") val, in("dx") VBE_DATA);
    val
}

#[no_mangle]
pub unsafe extern "C" fn krust_vga_vbe_init() -> bool {
    use crate::pci::PCI;

    // Check if VBE is available by reading ID
    let id = vbe_read(0);
    if id != 0xB0C5 {
        return false;
    }

    // Find VGA device on PCI (class 0x03, subclass 0x00)
    if let Some(vga) = PCI::enumerate_class(0x03, 0x00) {
        // Enable memory space access on PCI command register
        let mut cmd = PCI::config_read_word(vga.bus, vga.slot, vga.func, 0x04);
        cmd |= 1 << 1; // Memory Space Enable
        PCI::config_write_word(vga.bus, vga.slot, vga.func, 0x04, cmd);

        // Read BAR0 for framebuffer address
        let bar0 = PCI::read_bar(vga.bus, vga.slot, vga.func, 0);
        let fb_phys = (bar0 & 0xFFFFFFF0) as u64;

        if fb_phys == 0 {
            return false;
        }

        // Disable display while changing mode
        vbe_write(VBE_ENABLE, 0);

        // Set 1024x768x32
        vbe_write(VBE_XRES, 1024);
        vbe_write(VBE_YRES, 768);
        vbe_write(VBE_BPP, 32);

        // Set virtual framebuffer size (for double buffering)
        vbe_write(VBE_VIRT_WIDTH, 1024);
        vbe_write(VBE_VIRT_HEIGHT, 768 * 2);

        // Enable linear framebuffer (bit 0 = enable, bit 5 = linear FB)
        vbe_write(VBE_ENABLE, 0x41);

        // Read back actual resolution (QEMU may have adjusted)
        let w = vbe_read(VBE_XRES) as u32;
        let h = vbe_read(VBE_YRES) as u32;
        let bpp = vbe_read(VBE_BPP) as u32;

        // Map framebuffer into kernel address space (identity map covers first 4GB)
        let fb_ptr = fb_phys as *mut u8;

        VGA_MODE_FB = fb_ptr;
        VGA_MODE_W = w;
        VGA_MODE_H = h;
        VGA_MODE_PITCH = w * (bpp / 8);

        // Fill with black
        let total = (VGA_MODE_PITCH * VGA_MODE_H) as usize;
        let fb32 = fb_ptr as *mut u32;
        for i in 0..(total / 4) {
            ptr::write_volatile(fb32.add(i), 0x00000000);
        }

        // Init fb_console with the detected framebuffer
        crate::fb_console::fb_console_init(w, h, VGA_MODE_PITCH, bpp as u8, fb_ptr);

        return true;
    }

    false
}

pub unsafe fn vga_mode_fb() -> *mut u8 { VGA_MODE_FB }
pub unsafe fn vga_mode_w() -> u32 { VGA_MODE_W }
pub unsafe fn vga_mode_h() -> u32 { VGA_MODE_H }
pub unsafe fn vga_mode_pitch() -> u32 { VGA_MODE_PITCH }
pub unsafe fn vga_mode_active() -> bool { !VGA_MODE_FB.is_null() }

static mut ROW: usize = 0;
static mut COL: usize = 0;
static mut COLOR: u8 = 0x07; // light gray on black

fn vga_index(row: usize, col: usize) -> usize {
    row * WIDTH + col
}

fn make_entry(c: u8, color: u8) -> u16 {
    (color as u16) << 8 | c as u16
}

unsafe fn scroll() {
    if ROW >= HEIGHT {
        for r in 1..HEIGHT {
            for c in 0..WIDTH {
                let src = VGA_BUF.add(vga_index(r, c));
                let dst = VGA_BUF.add(vga_index(r - 1, c));
                ptr::write_volatile(dst, ptr::read_volatile(src));
            }
        }
        for c in 0..WIDTH {
            ptr::write_volatile(VGA_BUF.add(vga_index(HEIGHT - 1, c)), make_entry(b' ', COLOR));
        }
        ROW = HEIGHT - 1;
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_vga_init() {
    for i in 0..(WIDTH * HEIGHT) {
        ptr::write_volatile(VGA_BUF.add(i), make_entry(b' ', 0x07));
    }
    ROW = 0;
    COL = 0;
    COLOR = 0x07;
}

#[no_mangle]
pub unsafe extern "C" fn krust_vga_clear() {
    for i in 0..(WIDTH * HEIGHT) {
        ptr::write_volatile(VGA_BUF.add(i), make_entry(b' ', COLOR));
    }
    ROW = 0;
    COL = 0;
}

#[no_mangle]
pub unsafe extern "C" fn krust_vga_set_color(fg: u8, bg: u8) {
    COLOR = (bg << 4) | fg;
}

#[no_mangle]
pub unsafe extern "C" fn krust_vga_putchar(c: u8) {
    match c {
        b'\n' => { ROW += 1; COL = 0; }
        b'\r' => { COL = 0; }
        b'\t' => { COL = (COL + 8) & !7; }
        0x08 => { if COL > 0 { COL -= 1; } }
        _ => {
            ptr::write_volatile(VGA_BUF.add(vga_index(ROW, COL)), make_entry(c, COLOR));
            COL += 1;
            if COL >= WIDTH { COL = 0; ROW += 1; }
        }
    }
    scroll();
}

#[no_mangle]
pub unsafe extern "C" fn krust_vga_write(data: *const u8, len: usize) {
    for i in 0..len {
        krust_vga_putchar(ptr::read_volatile(data.add(i)));
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_vga_writestring(s: *const u8) {
    let mut i = 0;
    loop {
        let c = ptr::read_volatile(s.add(i));
        if c == 0 { break; }
        krust_vga_putchar(c);
        i += 1;
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_vga_get_cursor_row() -> usize { ROW }
#[no_mangle]
pub unsafe extern "C" fn krust_vga_get_cursor_col() -> usize { COL }

#[no_mangle]
pub unsafe extern "C" fn krust_vga_set_pos(x: usize, y: usize) {
    if x < WIDTH { COL = x; }
    if y < HEIGHT { ROW = y; }
}

#[no_mangle]
pub unsafe extern "C" fn krust_vga_writestring_color(s: *const u8, color: u8) {
    let old = COLOR;
    COLOR = color;
    krust_vga_writestring(s);
    COLOR = old;
}
