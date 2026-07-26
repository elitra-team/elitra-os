use core::ptr;

const WIDTH: usize = 128;
const HEIGHT: usize = 48;
const MAX_VTS: usize = 4;
const SCROLL_LINES: usize = 200;
const CONTENT_TOP: usize = 1;

#[repr(C)]
struct RTCInfo {
    second: u8,
    minute: u8,
    hour: u8,
    day: u8,
    month: u8,
    year: u16,
}

struct VTState {
    buffer: [u16; WIDTH * HEIGHT],
    scrollback: [u16; SCROLL_LINES * WIDTH],
    scrollback_head: i32,
    scrollback_count: i32,
    scroll_offset: i32,
    cursor_x: i32,
    cursor_y: i32,
    color: u8,
    active: bool,
}

extern "C" {
    fn krust_cmos_read_time() -> RTCInfo;
    fn krust_ps2kbd_getkey() -> i32;
    fn krust_ns16550_write_str(s: *const u8);
}

const fn make_color(fg: u8, bg: u8) -> u8 {
    fg | (bg << 4)
}

const fn vga4_to_fb_color(vga: u8) -> u32 {
    match vga & 0x0F {
        0x00 => 0x000000,
        0x01 => 0x0000AA,
        0x02 => 0x00AA00,
        0x03 => 0x00AAAA,
        0x04 => 0xAA0000,
        0x05 => 0xAA00AA,
        0x06 => 0xAA5500,
        0x07 => 0xAAAAAA,
        0x08 => 0x555555,
        0x09 => 0x5555FF,
        0x0A => 0x55FF55,
        0x0B => 0x55FFFF,
        0x0C => 0xFF5555,
        0x0D => 0xFF55FF,
        0x0E => 0xFFFF55,
        0x0F => 0xFFFFFF,
        _ => unreachable!(),
    }
}

const fn make_entry(c: u8, color: u8) -> u16 {
    (c as u16) | ((color as u16) << 8)
}

unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val);
}

fn make_entry_nonconst(c: u8, color: u8) -> u16 {
    (c as u16) | ((color as u16) << 8)
}

fn fb_console_present(_vt: usize, buffer: &[u16], _color: u8) {
    // If framebuffer is not available, write directly to VGA text buffer at 0xB8000
    if unsafe { crate::fb_console::fb_console_get_fb_ptr() }.is_null() {
        const VGA_TEXT: *mut u16 = 0xB8000 as *mut u16;
        let vga_cols = 80usize;
        let vga_rows = 25usize;
        unsafe {
            for row in 0..vga_rows {
                let dst = VGA_TEXT.add(row * vga_cols);
                let src = buffer.as_ptr().add(row * WIDTH);
                for col in 0..vga_cols {
                    let idx = row * WIDTH + col;
                    if idx < buffer.len() {
                        core::ptr::write_volatile(dst.add(col), core::ptr::read_volatile(src.add(col)));
                    }
                }
            }
        }
        return;
    }

    let cols = unsafe { crate::fb_console::fb_console_get_cols() } as usize;
    let rows = unsafe { crate::fb_console::fb_console_get_rows() } as usize;
    
    for row in 0..rows {
        for col in 0..cols {
            let idx = row * WIDTH + col;
            if idx < buffer.len() {
                let entry = buffer[idx];
                let c = (entry & 0xFF) as u8;
                let vga_color = ((entry >> 8) & 0xFF) as u8;
                let _fb_color = vga4_to_fb_color(vga_color);
                
                let x = col as u32;
                let y = row as u32;
                let fg = vga4_to_fb_color(vga_color & 0x0F);
                let bg = vga4_to_fb_color(((vga_color >> 4) & 0x0F) as u8);
                
                unsafe {
                    crate::fb_console::fb_console_draw_char_px(x * 8, y * 16, c, fg, bg);
                }
            }
        }
    }
}

static mut VTS: core::mem::MaybeUninit<[VTState; MAX_VTS]> = core::mem::MaybeUninit::uninit();
static mut ACTIVE: usize = 0;

unsafe fn vts_mut() -> &'static mut [VTState; MAX_VTS] {
    &mut *VTS.as_mut_ptr()
}

fn vt_mut(vt: usize) -> &'static mut VTState {
    unsafe { &mut (*VTS.as_mut_ptr())[vt] }
}

// --- Private helpers ---

unsafe fn push_scrollback(vt: usize, line: *const u16) {
    let s = vt_mut(vt);
    let idx = (s.scrollback_head as usize) * WIDTH;
    ptr::copy_nonoverlapping(line, s.scrollback.as_mut_ptr().add(idx), WIDTH);
    s.scrollback_head = (s.scrollback_head + 1) % (SCROLL_LINES as i32);
    if s.scrollback_count < SCROLL_LINES as i32 {
        s.scrollback_count += 1;
    }
}

unsafe fn fb_console_draw_scrollback(vt: usize, offset: i32) {
    let s = vt_mut(vt);
    let cols = crate::fb_console::fb_console_get_cols() as usize;
    let rows = crate::fb_console::fb_console_get_rows() as usize;
    let count = s.scrollback_count;
    let start_line = if offset <= 0 { 0 } else { count - offset };
    
    for row in CONTENT_TOP..rows {
        let sb_line = start_line + (row - CONTENT_TOP) as i32;
        if sb_line < count {
            let sb_idx = (((s.scrollback_head - count + sb_line + SCROLL_LINES as i32) % SCROLL_LINES as i32) as usize) * WIDTH;
            let line_ptr = s.scrollback.as_ptr().add(sb_idx);
            
            for col in 0..cols {
                let idx = row * WIDTH + col;
                if idx < s.buffer.len() {
                    let entry = ptr::read_volatile(line_ptr.add(col));
                    let c = (entry & 0xFF) as u8;
                    let vga_color = ((entry >> 8) & 0xFF) as u8;
                    let fg = vga4_to_fb_color(vga_color & 0x0F);
                    let bg = vga4_to_fb_color(((vga_color >> 4) & 0x0F) as u8);
                    
                    if c != b' ' {
                        let x = col as u32;
                        let y = row as u32;
                        unsafe {
                            crate::fb_console::fb_console_draw_char_px(x * 8, y * 16, c, fg, bg);
                        }
                    }
                }
            }
        } else {
            // Draw from current buffer
            let line_ptr = s.buffer.as_ptr().add(row * WIDTH);
            for col in 0..cols {
                let idx = row * WIDTH + col;
                if idx < s.buffer.len() {
                    let entry = ptr::read_volatile(line_ptr.add(col));
                    let c = (entry & 0xFF) as u8;
                    let vga_color = ((entry >> 8) & 0xFF) as u8;
                    let fg = vga4_to_fb_color(vga_color & 0x0F);
                    let bg = vga4_to_fb_color(((vga_color >> 4) & 0x0F) as u8);
                    
                    if c != b' ' {
                        let x = col as u32;
                        let y = row as u32;
                        unsafe {
                            crate::fb_console::fb_console_draw_char_px(x * 8, y * 16, c, fg, bg);
                        }
                    }
                }
            }
        }
    }
}

unsafe fn scroll_up(vt: usize) {
    let s = vt_mut(vt);
    push_scrollback(vt, s.buffer.as_ptr().add(CONTENT_TOP * WIDTH));
    for y in (CONTENT_TOP + 1)..HEIGHT {
        let dst = s.buffer.as_mut_ptr().add((y - 1) * WIDTH);
        let src = s.buffer.as_ptr().add(y * WIDTH);
        ptr::copy_nonoverlapping(src, dst, WIDTH);
    }
    let blank = make_entry_nonconst(b' ', s.color);
    for x in 0..WIDTH {
        *s.buffer.as_mut_ptr().add((HEIGHT - 1) * WIDTH + x) = blank;
    }
    
    // Update framebuffer
    fb_console_present(vt, &s.buffer, s.color);
    if vt == ACTIVE {
        unsafe { crate::fb_console::fb_console_draw_status_bar(ACTIVE as u32); }
    }
}

unsafe fn render_scrollback(vt: usize) {
    let s = vt_mut(vt);
    if s.scroll_offset <= 0 { return; }
    let count = s.scrollback_count;
    let offset = s.scroll_offset;
    let _start_line = if offset <= 0 { 0 } else { count - offset };
    
    fb_console_draw_scrollback(vt, offset);
    if vt == ACTIVE {
        unsafe { crate::fb_console::fb_console_draw_status_bar(ACTIVE as u32); }
    }
}

unsafe fn restore_live(vt: usize) {
    let s = vt_mut(vt);
    s.scroll_offset = 0;
    fb_console_present(vt, &s.buffer, s.color);
    if vt == ACTIVE {
        unsafe { crate::fb_console::fb_console_draw_status_bar(ACTIVE as u32); }
    }
}

unsafe fn update_cursor_inner() {
    // No cursor update needed for framebuffer
    // The fb_console handles cursor blinking
}

unsafe fn draw_status_bar_inner() {
    // This is now handled by fb_console_draw_status_bar
}

// --- Public API ---

#[no_mangle]
pub unsafe extern "C" fn krust_terminal_init() {
    let def = make_color(7, 0);
    let vts = vts_mut();
    for i in 0..MAX_VTS {
        let vt = &mut vts[i];
        let blank = make_entry_nonconst(b' ', def);
        for j in 0..(WIDTH * HEIGHT) {
            vt.buffer[j] = blank;
        }
        for j in 0..(SCROLL_LINES * WIDTH) {
            vt.scrollback[j] = blank;
        }
        vt.scrollback_head = 0;
        vt.scrollback_count = 0;
        vt.scroll_offset = 0;
        vt.cursor_x = 0;
        vt.cursor_y = CONTENT_TOP as i32;
        vt.color = def;
        vt.active = false;
    }
    vts[1].color = make_color(2, 0);
    vts[2].color = make_color(3, 0);
    vts[3].color = make_color(5, 0);
    ACTIVE = 0;
    vts[0].active = true;
    // Initialize framebuffer console
    let fb_width = crate::fb_console::fb_console_get_width();
    let fb_height = crate::fb_console::fb_console_get_height();
    let fb_pitch = crate::fb_console::fb_console_get_pitch();
    let fb_bpp = crate::fb_console::fb_console_get_bpp();
    let fb_ptr = crate::fb_console::fb_console_get_fb_ptr();
    
    if !fb_ptr.is_null() {
        crate::fb_console::fb_console_init(fb_width, fb_height, fb_pitch, fb_bpp, fb_ptr);
        fb_console_present(0, &vts[0].buffer, vts[0].color);
        crate::fb_console::fb_console_draw_status_bar(ACTIVE as u32);
    }
    
    krust_ns16550_write_str(b"terminal: 4 VTs, F1-F4 switch, PgUp/PgDn scroll\0" as *const u8);
}

#[no_mangle]
pub unsafe extern "C" fn krust_terminal_putchar(c: u8) {
    let vt = ACTIVE;
    let s = vt_mut(vt);
    if s.scroll_offset > 0 {
        restore_live(vt);
    }
    let c = c as char;
    match c {
        '\n' => {
            s.cursor_x = 0;
            s.cursor_y += 1;
            if s.cursor_y >= HEIGHT as i32 {
                scroll_up(vt);
                s.cursor_y = (HEIGHT - 1) as i32;
            }
            update_cursor_inner();
            return;
        }
        '\r' => {
            s.cursor_x = 0;
            update_cursor_inner();
            return;
        }
        '\t' => {
            loop {
                krust_terminal_putchar(b' ');
                if s.cursor_x % 8 == 0 { break; }
            }
            return;
        }
        '\x08' => {
            if s.cursor_x > 0 {
                s.cursor_x -= 1;
            } else if s.cursor_y > CONTENT_TOP as i32 {
                s.cursor_y -= 1;
                s.cursor_x = (WIDTH - 1) as i32;
            }
            let idx = (s.cursor_y as usize) * WIDTH + (s.cursor_x as usize);
            s.buffer[idx] = make_entry_nonconst(b' ', s.color);
            fb_console_present(vt, &s.buffer, s.color);
            return;
        }
        _ => {
            let idx = (s.cursor_y as usize) * WIDTH + (s.cursor_x as usize);
            s.buffer[idx] = make_entry_nonconst(c as u8, s.color);
            fb_console_present(vt, &s.buffer, s.color);
            s.cursor_x += 1;
            if s.cursor_x >= WIDTH as i32 {
                s.cursor_x = 0;
                s.cursor_y += 1;
                if s.cursor_y >= HEIGHT as i32 {
                    scroll_up(vt);
                    s.cursor_y = (HEIGHT - 1) as i32;
                }
            }
            fb_console_present(vt, &s.buffer, s.color);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_terminal_write(data: *const u8, len: usize) {
    for i in 0..len {
        krust_terminal_putchar(ptr::read_volatile(data.add(i)));
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_terminal_writestring(s: *const u8) {
    let mut p = s;
    loop {
        let c = ptr::read_volatile(p);
        if c == 0 { break; }
        krust_terminal_putchar(c);
        p = p.add(1);
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_terminal_writestring_color(s: *const u8, color: u8) {
    let vt = ACTIVE;
    let old = vt_mut(vt).color;
    vt_mut(vt).color = color;
    krust_terminal_writestring(s);
    vt_mut(vt).color = old;
}

#[no_mangle]
pub unsafe extern "C" fn krust_terminal_set_color(color: u8) {
    vt_mut(ACTIVE).color = color;
}

#[no_mangle]
pub unsafe extern "C" fn krust_terminal_clear() {
    let vt = ACTIVE;
    let s = vt_mut(vt);
    s.scroll_offset = 0;
    let entry = make_entry_nonconst(b' ', s.color);
    for i in 0..(WIDTH * HEIGHT) {
        s.buffer[i] = entry;
    }
    s.cursor_x = 0;
    s.cursor_y = CONTENT_TOP as i32;
        fb_console_present(vt, &s.buffer, s.color);
    draw_status_bar_inner();
    update_cursor_inner();
}

#[no_mangle]
pub unsafe extern "C" fn krust_terminal_set_pos(x: i32, y: i32) {
    let s = vt_mut(ACTIVE);
    if x >= 0 && x < WIDTH as i32 { s.cursor_x = x; }
    if y >= CONTENT_TOP as i32 && y < HEIGHT as i32 { s.cursor_y = y; }
    update_cursor_inner();
}

#[no_mangle]
pub unsafe extern "C" fn krust_terminal_get_pos(x: *mut i32, y: *mut i32) {
    let s = vt_mut(ACTIVE);
    ptr::write(x, s.cursor_x);
    ptr::write(y, s.cursor_y);
}

#[no_mangle]
pub unsafe extern "C" fn krust_terminal_switch_vt(vt: i32) {
    if vt < 0 || vt >= MAX_VTS as i32 || vt as usize == ACTIVE { return; }
    fb_console_present(ACTIVE, &vt_mut(ACTIVE).buffer, vt_mut(ACTIVE).color);
    ACTIVE = vt as usize;
    let s = vt_mut(ACTIVE);
    if s.scroll_offset > 0 {
        render_scrollback(ACTIVE);
    } else {
    fb_console_present(vt as usize, &s.buffer, s.color);
    }
    draw_status_bar_inner();
    update_cursor_inner();
}

#[no_mangle]
pub unsafe extern "C" fn krust_terminal_active_vt() -> i32 {
    ACTIVE as i32
}

#[no_mangle]
pub unsafe extern "C" fn krust_terminal_process_key(key: i32) -> i32 {
    if key >= 256 && key <= 259 {
        krust_terminal_switch_vt(key - 256);
        draw_status_bar_inner();
        return 1;
    }
    let vt = ACTIVE;
    let s = vt_mut(vt);
    if key == 268 {
        if s.scrollback_count > 0 {
            s.scroll_offset += 1;
            if s.scroll_offset > s.scrollback_count { s.scroll_offset = s.scrollback_count; }
            render_scrollback(vt);
        }
        return 1;
    }
    if key == 269 {
        if s.scroll_offset > 0 {
            s.scroll_offset -= 1;
            if s.scroll_offset <= 0 { restore_live(vt); }
            else { render_scrollback(vt); }
        }
        return 1;
    }
    if key == 274 {
        if s.scrollback_count > 0 && s.scroll_offset != s.scrollback_count {
            s.scroll_offset = s.scrollback_count;
            render_scrollback(vt);
        }
        return 1;
    }
    if key == 275 {
        if s.scroll_offset > 0 {
            s.scroll_offset = 0;
            restore_live(vt);
        }
        return 1;
    }
    if s.scroll_offset > 0 {
        s.scroll_offset = 0;
        restore_live(vt);
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_terminal_readline(buf: *mut u8, max: i32) {
    draw_status_bar_inner();
    let mut pos = 0i32;
    while pos < max - 1 {
        let k = krust_ps2kbd_getkey();
        if krust_terminal_process_key(k) != 0 { continue; }
        if k >= 270 && k <= 273 { continue; }
        if k >= 0 && k < 256 {
            let c = k as u8;
            if c == b'\n' {
                krust_terminal_putchar(b'\n');
                ptr::write(buf.add(pos as usize), 0);
                return;
            }
            if c == b'\x08' {
                if pos > 0 {
                    pos -= 1;
                    krust_terminal_putchar(b'\x08');
                    krust_terminal_putchar(b' ');
                    krust_terminal_putchar(b'\x08');
                }
                continue;
            }
            ptr::write(buf.add(pos as usize), c);
            pos += 1;
            krust_terminal_putchar(c);
        }
    }
    ptr::write(buf.add(pos as usize), 0);
}

#[no_mangle]
pub unsafe extern "C" fn krust_terminal_update_status_bar() {
    draw_status_bar_inner();
}
