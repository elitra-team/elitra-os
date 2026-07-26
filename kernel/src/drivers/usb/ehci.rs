use core::ptr;

const MAX_DEV: usize = 8;
const MAX_HID: usize = 4;
const RETRIES: u32 = 1000000;

#[repr(C)]
struct DevReq { rt: u8, rq: u8, wv: u16, wi: u16, wl: u16 }

#[repr(C, packed)]
struct DevDesc { len: u8, typ: u8, usb: u16, cls: u8, sub: u8, proto: u8, maxp: u8, vid: u16, pid: u16, dev: u16, man: u8, prod: u8, ser: u8, cn: u8 }

#[repr(C, packed)]
struct CfgDesc { len: u8, typ: u8, total: u16, ni: u8, val: u8, iconf: u8, attr: u8, maxpwr: u8 }

#[repr(C, packed)]
struct IfDesc { len: u8, typ: u8, num: u8, alt: u8, ne: u8, cls: u8, sub: u8, proto: u8, iface: u8 }

#[repr(C, packed)]
struct EpDesc { len: u8, typ: u8, addr: u8, attr: u8, maxsz: u16, ival: u8 }

#[repr(C, align(32))]
struct Qh { next: u32, alt: u32, epchar: u32, caps: u32, curr: u32, _rsvd: [u32; 3] }

#[repr(C, align(32))]
struct Qtd { next: u32, alt: u32, token: u32, buf: [u32; 5], _rsvd: u32 }

#[derive(Clone, Copy)]
struct Device { addr: u8, port: u8, maxp: u8, speed: u8, active: bool }

struct Hid {
    di: usize, iface: u8, ep: u8, epsz: u16,
    proto: u8, active: bool, toggle: u32,
    qh: *mut Qh, qhp: u32,
    td: *mut Qtd, tdp: u32,
    buf: [u8; 64],
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct UsbEvent { pub typ: u8, pub mods: u8, pub key: u8, pub dx: i8, pub dy: i8, pub btn: u8 }

const CAPLENGTH: u64 = 0x00;
const HCSPARAMS: u64 = 0x04;
const HCCPARAMS: u64 = 0x08;
const SP_NPORTS: u32 = 0x0000000F;
const CP_64BIT: u32 = 0x00000001;

const CMD: u64       = 0x00;
const STS: u64       = 0x04;
const FRINDEX: u64   = 0x0C;
const PERIODICLIST: u64 = 0x14;
const ASYNCLISTADDR: u64 = 0x18;
const CONFIGFLAG: u64 = 0x40;
const PORTSC: u64    = 0x44;

const CMD_RUN: u32    = 0x00000001;
const CMD_HCRST: u32  = 0x00000002;
const CMD_ASE: u32    = 0x00000008;
const CMD_PSE: u32    = 0x00000010;
const CMD_IAAD: u32   = 0x00000040;
const CMD_CFG: u32    = 0x00000080;

const STS_HALTED: u32 = 0x00001000;
const STS_AA: u32     = 0x00000020;

const P_CCS: u32    = 0x00000001;
const P_CSC: u32    = 0x00000002;
const P_PED: u32    = 0x00000004;
const P_FPR: u32    = 0x00000010;
const P_RST: u32    = 0x00000100;
const P_LL: u32     = 0x00000C00;
const LL_FULL: u32  = 0x00000000;
const LL_LOW: u32   = 0x00000400;
const LL_HIGH: u32  = 0x00000800;

const QH_DEVADDR: u32 = 0x0000007F;
const QH_INACT: u32   = 0x00000080;
const QH_ENDPT: u32   = 0x00000F00;
const QH_EPS: u32     = 0x00003000;
const EPS_FULL: u32   = 0x00000000;
const EPS_LOW: u32    = 0x00001000;
const EPS_HIGH: u32   = 0x00002000;
const QH_DTC: u32     = 0x00004000;
const QH_RL: u32      = 0x000F0000;
const QH_C: u32       = 0x00100000;
const QH_H: u32       = 0x00200000;

const QT_TOGGLE: u32  = 0x80000000;
const QT_TOTAL: u32   = 0x7FFF0000;
const QT_IOC: u32     = 0x00008000;
const QT_CERR: u32    = 0x00000060;
const QT_PID: u32     = 0x0000001C;
const QT_STS: u32     = 0x00000003;

const QT_PID_IN: u32   = 0x00000014;
const QT_PID_OUT: u32  = 0x00000010;
const QT_PID_SETUP: u32 = 0x00000000;
const QT_ACTIVE: u32    = 0x00000000;

const LK_TERM: u32 = 0x00000001;

const REQ_GET_DESC: u8 = 6;
const REQ_SET_ADDR: u8 = 5;
const REQ_SET_CFG: u8 = 9;
const DT_DEV: u16 = 1;
const DT_CFG: u16 = 2;
const HID_CLS: u8 = 3;
const HID_KBD: u8 = 1;
const HID_MOU: u8 = 2;

static mut OP: u64 = 0;
static mut DEVS: [Device; MAX_DEV] = unsafe { core::mem::zeroed() };
static mut NDEV: usize = 0;
static mut NADDR: u8 = 1;
static mut HIDS: [Hid; MAX_HID] = unsafe { core::mem::zeroed() };
static mut NHID: usize = 0;
static mut EVS: [UsbEvent; 16] = unsafe { core::mem::zeroed() };
static mut EVH: usize = 0;
static mut EVT: usize = 0;

// Async schedule QH (the static head)
static mut ANCHOR_QH: *mut Qh = ptr::null_mut();
static mut ANCHOR_QHP: u32 = 0;
static mut WP: *mut u8 = ptr::null_mut();
static mut WPP: u32 = 0;

extern "C" {
    fn krust_pmm_alloc_frame() -> usize;
    fn krust_pmm_free_frame(f: usize);
    fn krust_map_mmio(phys: u64, size: u64) -> u64;
    fn krust_serial_write(p: *const u8, l: usize);
    fn krust_serial_putchar(c: u8);
}

unsafe fn dbg(s: &[u8]) { krust_serial_write(s.as_ptr(), s.len()); }
unsafe fn dhex(v: u32) {
    let h = b"0123456789ABCDEF";
    krust_serial_putchar(b'0'); krust_serial_putchar(b'x');
    for i in (0..8).rev() { krust_serial_putchar(h[((v >> (i*4)) & 0xF) as usize]); }
}
unsafe fn dln() { krust_serial_putchar(b'\n'); }
unsafe fn ds(s: &[u8], v: u32) { dbg(s); dhex(v); dln(); }

unsafe fn rr(off: u64) -> u32 { ptr::read_volatile((OP + off) as *mut u32) }
unsafe fn ww(off: u64, v: u32) { ptr::write_volatile((OP + off) as *mut u32, v) }

unsafe fn page_alloc() -> (u32, *mut u8) {
    let f = krust_pmm_alloc_frame();
    if f == !0 { (0, ptr::null_mut()) } else { ((f * 4096) as u32, (f * 4096) as *mut u8) }
}

unsafe fn wait_qtd(td: *const Qtd) -> bool {
    for _ in 0..RETRIES {
        let t = ptr::read_volatile(&(*td).token);
        if t & 3 != 0 { return (t & 3) == 1; }
        rr(FRINDEX);
    }
    false
}

fn qh_char(dev: u8, ep: u8, speed: u8, maxp: u16) -> u32 {
    let sp = match speed { 2 => EPS_HIGH, 1 => EPS_LOW, _ => EPS_FULL };
    (dev as u32) | (ep as u32) << 8 | sp | QH_DTC | (0x0F << 16) | ((maxp as u32) << 16)
}

unsafe fn do_ctrl(dev_addr: u8, req: *const DevReq, data: *mut u8, dlen: u16) -> bool {
    let dir_in = (*req).rt & 0x80 != 0;
    let has_data = dlen > 0;
    let maxp = if dev_addr == 0 { 8 } else { DEVS[(dev_addr - 1) as usize].maxp as u16 };
    let speed = if dev_addr == 0 { 2 } else { DEVS[(dev_addr - 1) as usize].speed };
    ptr::write_bytes(WP, 0, 4096);

    // Layout in WP page:
    // [0..32]   QH for this transfer
    // [32..64]  qTD setup
    // [64..96]  qTD data (optional)
    // [96..128] qTD status
    // [128..136] setup packet
    // [256..]   data buffer
    let qh = WP as *mut Qh;
    let qhp = WPP;
    let td_s = WP.add(32) as *mut Qtd;
    let sp = WPP + 32;
    let td_d = WP.add(64) as *mut Qtd;
    let dp = WPP + 64;
    let td_st = WP.add(96) as *mut Qtd;
    let stp = WPP + 96;

    // Setup TD
    ptr::copy_nonoverlapping(req as *const u8, WP.add(128), 8);
    ptr::write_volatile(ptr::addr_of_mut!((*td_s).next), if has_data || dir_in { dp } else { stp });
    ptr::write_volatile(ptr::addr_of_mut!((*td_s).alt), LK_TERM);
    ptr::write_volatile(ptr::addr_of_mut!((*td_s).token), ((8 & 0x7FFF) << 16) | QT_CERR | QT_PID_SETUP);
    ptr::write_volatile(ptr::addr_of_mut!((*td_s).buf[0]), WPP + 128);
    for i in 1..5 { ptr::write_volatile(ptr::addr_of_mut!((*td_s).buf[i]), 0); }
    ptr::write_volatile(ptr::addr_of_mut!((*td_s)._rsvd), 0);

    // Data TD
    if has_data {
        let dpid = if dir_in { QT_PID_IN } else { QT_PID_OUT };
        let dbuf = if dir_in { WPP + 256 } else { WPP + 256 };
        if !dir_in { ptr::copy_nonoverlapping(data, WP.add(256), dlen as usize); }
        ptr::write_volatile(ptr::addr_of_mut!((*td_d).next), stp);
        ptr::write_volatile(ptr::addr_of_mut!((*td_d).alt), LK_TERM);
        ptr::write_volatile(ptr::addr_of_mut!((*td_d).token), ((dlen as u32 & 0x7FFF) << 16) | QT_CERR | dpid | 0x80000000);
        ptr::write_volatile(ptr::addr_of_mut!((*td_d).buf[0]), dbuf);
        for i in 1..5 { ptr::write_volatile(ptr::addr_of_mut!((*td_d).buf[i]), 0); }
        ptr::write_volatile(ptr::addr_of_mut!((*td_d)._rsvd), 0);
    }

    // Status TD
    let spid = if dir_in && has_data { QT_PID_OUT } else { QT_PID_IN };
    ptr::write_volatile(ptr::addr_of_mut!((*td_st).next), LK_TERM);
    ptr::write_volatile(ptr::addr_of_mut!((*td_st).alt), LK_TERM);
    ptr::write_volatile(ptr::addr_of_mut!((*td_st).token), (0 << 16) | QT_CERR | spid | 0x80000000 | QT_IOC);
    ptr::write_volatile(ptr::addr_of_mut!((*td_st).buf[0]), 0);
    for i in 1..5 { ptr::write_volatile(ptr::addr_of_mut!((*td_st).buf[i]), 0); }
    ptr::write_volatile(ptr::addr_of_mut!((*td_st)._rsvd), 0);

    // QH: head of this control transfer
    ptr::write_volatile(ptr::addr_of_mut!((*qh).next), LK_TERM);
    ptr::write_volatile(ptr::addr_of_mut!((*qh).alt), sp);
    ptr::write_volatile(ptr::addr_of_mut!((*qh).epchar), qh_char(dev_addr, 0, speed, maxp) | QH_C);
    ptr::write_volatile(ptr::addr_of_mut!((*qh).caps), (0x0F << 12) | (0x01 << 0)); // nak=15, smask=1
    ptr::write_volatile(ptr::addr_of_mut!((*qh).curr), 0);

    // Link QH into async list (after anchor)
    ptr::write_volatile(ptr::addr_of_mut!((*ANCHOR_QH).next), qhp);
    ptr::write_volatile(ptr::addr_of_mut!((*ANCHOR_QH).caps), (0x0F << 12) | (0x00 << 0)); // anchor has no smask
    ww(CMD, rr(CMD) | CMD_ASE);
    ww(CMD, rr(CMD) | CMD_IAAD);
    for _ in 0..10000 { if rr(STS) & STS_AA != 0 { ww(STS, STS_AA); break; } }

    let ok = wait_qtd(&*td_st);
    ptr::write_volatile(ptr::addr_of_mut!((*ANCHOR_QH).next), LK_TERM);
    ww(CMD, rr(CMD) & !CMD_ASE);

    if dir_in && has_data && ok {
        ptr::copy_nonoverlapping(WP.add(256), data, dlen as usize);
    }
    ok
}

unsafe fn get_dev_desc(addr: u8, buf: *mut u8, len: u16) -> bool {
    let req = DevReq { rt: 0x80, rq: REQ_GET_DESC, wv: DT_DEV << 8, wi: 0, wl: len };
    do_ctrl(addr, &req, buf, len)
}

unsafe fn get_cfg_desc(addr: u8, buf: *mut u8, len: u16) -> bool {
    let req = DevReq { rt: 0x80, rq: REQ_GET_DESC, wv: DT_CFG << 8, wi: 0, wl: len };
    do_ctrl(addr, &req, buf, len)
}

unsafe fn set_dev_addr(addr: u8) -> bool {
    let req = DevReq { rt: 0x00, rq: REQ_SET_ADDR, wv: addr as u16, wi: 0, wl: 0 };
    do_ctrl(0, &req, ptr::null_mut(), 0)
}

unsafe fn set_dev_cfg(addr: u8, val: u8) -> bool {
    let req = DevReq { rt: 0x00, rq: REQ_SET_CFG, wv: val as u16, wi: 0, wl: 0 };
    do_ctrl(addr, &req, ptr::null_mut(), 0)
}

unsafe fn set_hid_proto(addr: u8, iface: u8, proto: u8) -> bool {
    let req = DevReq { rt: 0x01 | 0x20, rq: 0x0B, wv: proto as u16, wi: iface as u16, wl: 0 };
    do_ctrl(addr, &req, ptr::null_mut(), 0)
}

unsafe fn ehci_reset() {
    ww(CMD, CMD_HCRST);
    for _ in 0..200000 { if rr(CMD) & CMD_HCRST == 0 { break; } }
}

unsafe fn init_ehci(mmio: *mut u8) -> bool {
    let caplen = ptr::read_volatile(mmio) as u64;
    OP = mmio as u64 + caplen;
    let hcsp = ptr::read_volatile((mmio as u64 + HCSPARAMS) as *mut u32);
    let hccp = ptr::read_volatile((mmio as u64 + HCCPARAMS) as *mut u32);
    let nports = hcsp & SP_NPORTS;
    let is_64 = (hccp & CP_64BIT) != 0;
    ds(b"ehci: nports=", nports);
    ds(b"ehci: 64bit=", is_64 as u32);

    if is_64 { ww(0x10, 0); }

    ehci_reset();
    ww(CMD, CMD_RUN | CMD_CFG);
    for _ in 0..200000 { if rr(STS) & STS_HALTED == 0 { break; } }
    if rr(STS) & STS_HALTED != 0 { ds(b"ehci: still halted", 0); return false; }

    // Anchor QH
    let (ahp, ahv) = page_alloc();
    if ahv.is_null() { return false; }
    ptr::write_bytes(ahv, 0, 4096);
    ANCHOR_QHP = ahp;
    ANCHOR_QH = ahv as *mut Qh;
    ptr::write_volatile(ptr::addr_of_mut!((*ANCHOR_QH).next), LK_TERM);
    ptr::write_volatile(ptr::addr_of_mut!((*ANCHOR_QH).alt), LK_TERM);
    ptr::write_volatile(ptr::addr_of_mut!((*ANCHOR_QH).epchar), QH_H | QH_INACT | QH_C); // H bit = head of list
    ptr::write_volatile(ptr::addr_of_mut!((*ANCHOR_QH).caps), 0x0F << 12);
    ptr::write_volatile(ptr::addr_of_mut!((*ANCHOR_QH).curr), 0);
    ww(ASYNCLISTADDR, ahp);

    // Workspace
    let (wpp, wpv) = page_alloc();
    if wpv.is_null() { return false; }
    WPP = wpp; WP = wpv;
    ptr::write_bytes(wpv, 0, 4096);

    // Reset all ports
    for p in 0..nports {
        let ps = PORTSC + p as u64 * 4;
        ww(ps, rr(ps) | P_RST);
        for _ in 0..200000 { if rr(ps) & P_RST == 0 { break; } }
    }

    ww(CONFIGFLAG, 1);
    dbg(b"ehci: init ok\n");
    true
}

unsafe fn enum_device(port: u8) {
    if NDEV >= MAX_DEV { return; }
    let idx = NDEV;
    let ps = PORTSC + port as u64 * 4;
    let sc = rr(ps);
    let speed = ((sc & P_LL) >> 10) as u8;
    ds(b"ehci: port ", port as u32); ds(b" speed=", speed as u32);

    DEVS[idx] = Device { addr: 0, port, maxp: 8, speed, active: true };
    let dbuf = WP.add(256);

    if !get_dev_desc(0, dbuf, 8) { DEVS[idx].active = false; return; }
    let maxp = (*(dbuf as *const DevDesc)).maxp;
    DEVS[idx].maxp = maxp;
    ds(b"ehci: maxp=", maxp as u32);

    let addr = NADDR; NADDR += 1;
    if !set_dev_addr(addr) { DEVS[idx].active = false; return; }
    DEVS[idx].addr = addr;
    wait_ms(2);
    ds(b"ehci: addr=", addr as u32);

    if !get_dev_desc(addr, dbuf, 18) { DEVS[idx].active = false; return; }
    if !get_cfg_desc(addr, dbuf, 9) { DEVS[idx].active = false; return; }
    let total = (*(dbuf as *const CfgDesc)).total;
    if total > 512 { DEVS[idx].active = false; return; }
    if !get_cfg_desc(addr, dbuf, total) { DEVS[idx].active = false; return; }
    if !set_dev_cfg(addr, 1) { DEVS[idx].active = false; return; }

    let mut off: usize = 0;
    while off + 2 < total as usize {
        let len = *dbuf.add(off) as usize;
        let typ = *dbuf.add(off + 1);
        if len == 0 || off + len > total as usize { break; }
        if typ == 4 {
            let iface = &*(dbuf.add(off) as *const IfDesc);
            if iface.cls == HID_CLS {
                ds(b"ehci: HID proto=", iface.proto as u32);
                setup_hid(idx, iface, dbuf, off, total as usize);
            }
        }
        off += len;
    }

    NDEV += 1;
    dbg(b"ehci: enumerated\n");
}

unsafe fn setup_hid(di: usize, iface: &IfDesc, cfg: *mut u8, start: usize, total: usize) {
    if NHID >= MAX_HID { return; }
    let mut ep_in: u8 = 0;
    let mut ep_sz: u16 = 8;
    let mut off = start + iface.len as usize;
    while off + 2 < total {
        let len = *cfg.add(off) as usize;
        let typ = *cfg.add(off + 1);
        if len == 0 || off + len > total { break; }
        if typ == 5 {
            let ep = &*(cfg.add(off) as *const EpDesc);
            if ep.addr & 0x80 != 0 { ep_in = ep.addr; ep_sz = ep.maxsz; }
        }
        off += len;
    }
    if ep_in == 0 { return; }
    if ep_sz > 64 { ep_sz = 64; }

    let (pp, pv) = page_alloc();
    if pv.is_null() { return; }
    ptr::write_bytes(pv, 0, 4096);

    let qh = pv as *mut Qh;
    let td = pv.add(64) as *mut Qtd;
    let dev = &DEVS[di];
    let eid = ep_in & 0x7F;

    set_hid_proto(dev.addr, iface.num, 0);

    ptr::write_volatile(ptr::addr_of_mut!((*qh).next), LK_TERM);
    ptr::write_volatile(ptr::addr_of_mut!((*qh).alt), LK_TERM);
    ptr::write_volatile(ptr::addr_of_mut!((*qh).epchar), qh_char(dev.addr, eid, dev.speed, ep_sz) | QH_C);
    ptr::write_volatile(ptr::addr_of_mut!((*qh).caps), (0x0F << 12) | (0x01 << 0));
    ptr::write_volatile(ptr::addr_of_mut!((*qh).curr), 0);

    let hi = NHID;
    HIDS[hi] = Hid {
        di, iface: iface.num, ep: ep_in, epsz: ep_sz,
        proto: iface.proto, active: true, toggle: 0,
        qh, qhp: pp, td, tdp: pp + 64, buf: [0u8; 64],
    };
    rearm_hid(&mut HIDS[hi]);
    NHID += 1;
}

unsafe fn rearm_hid(hid: &mut Hid) {
    ptr::write_volatile(ptr::addr_of_mut!((*hid.td).next), LK_TERM);
    ptr::write_volatile(ptr::addr_of_mut!((*hid.td).alt), LK_TERM);
    ptr::write_volatile(ptr::addr_of_mut!((*hid.td).token), ((hid.epsz as u32 & 0x7FFF) << 16) | QT_CERR | QT_PID_IN | (if hid.toggle != 0 { 0x80000000 } else { 0 }) | QT_IOC);
    ptr::write_volatile(ptr::addr_of_mut!((*hid.td).buf[0]), hid.buf.as_ptr() as u32);
    for i in 1..5 { ptr::write_volatile(ptr::addr_of_mut!((*hid.td).buf[i]), 0); }
    ptr::write_volatile(ptr::addr_of_mut!((*hid.td)._rsvd), 0);
}

unsafe fn wait_ms(ms: u32) {
    let start = rr(FRINDEX);
    loop { if rr(FRINDEX).wrapping_sub(start) >= ms * 8 { break; } }
}

unsafe fn poll_port(port: u8) {
    let ps = PORTSC + port as u64 * 4;
    let sc = rr(ps);
    if sc & P_CSC != 0 {
        let ccs = sc & P_CCS;
        ww(ps, sc | P_CSC);
        if ccs != 0 {
            wait_ms(100);
            enum_device(port);
        }
    }
}

unsafe fn poll_hid(hid: &mut Hid) {
    if !hid.active { return; }
    let token = ptr::read_volatile(&(*hid.td).token);
    if token & 3 == 0 { return; }
    if token & 3 == 2 || token & 3 == 3 {
        hid.toggle ^= 1;
        rearm_hid(hid);
        return;
    }
    let len = ((token >> 16) & 0x7FFF) as usize;
    if len > 0 && len <= 8 {
        let ev = match hid.proto {
            HID_KBD => {
                UsbEvent { typ: 1, mods: hid.buf[0], key: hid.buf[2], dx: 0, dy: 0, btn: 0 }
            }
            HID_MOU => {
                UsbEvent { typ: 2, mods: 0, key: 0, dx: hid.buf[1] as i8, dy: hid.buf[2] as i8, btn: hid.buf[0] & 7 }
            }
            _ => UsbEvent { typ: 0, mods: 0, key: 0, dx: 0, dy: 0, btn: 0 },
        };
        if ev.typ != 0 {
            let n = (EVH + 1) & 15;
            if n != EVT { EVS[EVH] = ev; EVH = n; }
        }
    }
    hid.toggle ^= 1;
    rearm_hid(hid);
}

// === Bulk Transfers (for USB Mass Storage) ===

// Bulk OUT: send `len` bytes from `data` to device `addr` endpoint `ep`
#[no_mangle]
pub unsafe extern "C" fn krust_ehci_bulk_out(addr: u8, ep: u8, data: *const u8, len: usize) -> bool {
    if addr == 0 || data.is_null() || len == 0 { return false; }
    let dev_idx = (addr - 1) as usize;
    if dev_idx >= NDEV { return false; }
    let dev = &DEVS[dev_idx];
    let maxp = dev.maxp as u32;
    let speed = dev.speed;
    let eid = ep & 0x7F;

    // We need: a QH, qTDs, and a data buffer - all in one DMA page
    let (pp, pv) = page_alloc();
    if pv.is_null() { return false; }
    ptr::write_bytes(pv, 0, 4096);

    // Layout:
    // [0..32]    QH
    // [32..64]   qTD (active)
    // [128..4096] data buffer
    let qh = pv as *mut Qh;
    let qhp = pp;
    let td = pv.add(32) as *mut Qtd;
    let tdp = pp + 32;
    let data_buf = pv.add(128);
    let data_phys = pp + 128;

    // Copy data to DMA buffer (max one page)
    let copy_len = if len > 3968 { 3968 } else { len };
    ptr::copy_nonoverlapping(data, data_buf, copy_len);

    // Build qTD for OUT transfer
    ptr::write_volatile(ptr::addr_of_mut!((*td).next), LK_TERM);
    ptr::write_volatile(ptr::addr_of_mut!((*td).alt), LK_TERM);
    ptr::write_volatile(ptr::addr_of_mut!((*td).token),
        ((copy_len as u32 & 0x7FFF) << 16) | QT_CERR | QT_PID_OUT | 0x80000000 | QT_IOC);
    ptr::write_volatile(ptr::addr_of_mut!((*td).buf[0]), data_phys);
    for i in 1..5 { ptr::write_volatile(ptr::addr_of_mut!((*td).buf[i]), 0); }
    ptr::write_volatile(ptr::addr_of_mut!((*td)._rsvd), 0);

    // Build QH for this endpoint
    ptr::write_volatile(ptr::addr_of_mut!((*qh).next), LK_TERM);
    ptr::write_volatile(ptr::addr_of_mut!((*qh).alt), tdp);
    ptr::write_volatile(ptr::addr_of_mut!((*qh).epchar), qh_char(addr, eid, speed, maxp as u16));
    ptr::write_volatile(ptr::addr_of_mut!((*qh).caps), (0x0F << 12));
    ptr::write_volatile(ptr::addr_of_mut!((*qh).curr), 0);

    // Link into async schedule and run
    ptr::write_volatile(ptr::addr_of_mut!((*ANCHOR_QH).next), qhp);
    ptr::write_volatile(ptr::addr_of_mut!((*ANCHOR_QH).caps), (0x0F << 12) | (0x00 << 0));
    ww(CMD, rr(CMD) | CMD_ASE);
    ww(CMD, rr(CMD) | CMD_IAAD);
    for _ in 0..10000 { if rr(STS) & STS_AA != 0 { ww(STS, STS_AA); break; } }

    let ok = wait_qtd(&*td);

    // Cleanup
    ptr::write_volatile(ptr::addr_of_mut!((*ANCHOR_QH).next), LK_TERM);
    ww(CMD, rr(CMD) & !CMD_ASE);

    // Free the DMA page
    crate::pmm::krust_pmm_free_frame((pp / 4096) as usize);

    ok
}

// Bulk IN: receive `len` bytes into `buf` from device `addr` endpoint `ep`
#[no_mangle]
pub unsafe extern "C" fn krust_ehci_bulk_in(addr: u8, ep: u8, buf: *mut u8, len: usize) -> bool {
    if addr == 0 || buf.is_null() || len == 0 { return false; }
    let dev_idx = (addr - 1) as usize;
    if dev_idx >= NDEV { return false; }
    let dev = &DEVS[dev_idx];
    let maxp = dev.maxp as u32;
    let speed = dev.speed;
    let eid = ep & 0x7F;

    let mut offset = 0usize;
    let mut toggle = 0u32;
    let mut overall_ok = true;

    while offset < len && overall_ok {
        let remaining = len - offset;
        let chunk = if remaining > 3968 { 3968 } else { remaining };

        let (pp, pv) = page_alloc();
        if pv.is_null() { overall_ok = false; break; }
        ptr::write_bytes(pv, 0, 4096);

        let qh = pv as *mut Qh;
        let qhp = pp;
        let td = pv.add(32) as *mut Qtd;
        let tdp = pp + 32;
        let data_buf = pv.add(128);
        let data_phys = pp + 128;

        // Build qTD for IN transfer
        ptr::write_volatile(ptr::addr_of_mut!((*td).next), LK_TERM);
        ptr::write_volatile(ptr::addr_of_mut!((*td).alt), LK_TERM);
        ptr::write_volatile(ptr::addr_of_mut!((*td).token),
            ((chunk as u32 & 0x7FFF) << 16) | QT_CERR | QT_PID_IN
            | (if toggle != 0 { 0x80000000 } else { 0 }) | QT_IOC);
        ptr::write_volatile(ptr::addr_of_mut!((*td).buf[0]), data_phys);
        for i in 1..5 { ptr::write_volatile(ptr::addr_of_mut!((*td).buf[i]), 0); }
        ptr::write_volatile(ptr::addr_of_mut!((*td)._rsvd), 0);

        // Build QH
        ptr::write_volatile(ptr::addr_of_mut!((*qh).next), LK_TERM);
        ptr::write_volatile(ptr::addr_of_mut!((*qh).alt), tdp);
        ptr::write_volatile(ptr::addr_of_mut!((*qh).epchar), qh_char(addr, eid, speed, maxp as u16));
        ptr::write_volatile(ptr::addr_of_mut!((*qh).caps), (0x0F << 12));
        ptr::write_volatile(ptr::addr_of_mut!((*qh).curr), 0);

        // Link into async and run
        ptr::write_volatile(ptr::addr_of_mut!((*ANCHOR_QH).next), qhp);
        ptr::write_volatile(ptr::addr_of_mut!((*ANCHOR_QH).caps), (0x0F << 12) | (0x00 << 0));
        ww(CMD, rr(CMD) | CMD_ASE);
        ww(CMD, rr(CMD) | CMD_IAAD);
        for _ in 0..10000 { if rr(STS) & STS_AA != 0 { ww(STS, STS_AA); break; } }

        let ok = wait_qtd(&*td);

        ptr::write_volatile(ptr::addr_of_mut!((*ANCHOR_QH).next), LK_TERM);
        ww(CMD, rr(CMD) & !CMD_ASE);

        if ok {
            ptr::copy_nonoverlapping(data_buf, buf.add(offset), chunk);
        } else {
            overall_ok = false;
        }

        crate::pmm::krust_pmm_free_frame((pp / 4096) as usize);
        offset += chunk;
        toggle ^= 1;
    }

    overall_ok
}

// Enumerate all EHCI devices for USB Mass Storage
#[no_mangle]
pub unsafe extern "C" fn krust_ehci_enumerate_storage() {
    for i in 0..NDEV {
        if !DEVS[i].active { continue; }
        let addr = DEVS[i].addr;
        let dbuf = WP.add(256);
        if !get_cfg_desc(addr, dbuf, 9) { continue; }
        let total = (*(dbuf as *const CfgDesc)).total;
        if total > 512 { continue; }
        if !get_cfg_desc(addr, dbuf, total) { continue; }

        let mut off: usize = 0;
        while off + 2 < total as usize {
            let len = *dbuf.add(off) as usize;
            let typ = *dbuf.add(off + 1);
            if len == 0 || off + len > total as usize { break; }
            if typ == 4 {  // DT_IFACE
                let iface = &*(dbuf.add(off) as *const IfDesc);
                if iface.cls == crate::usb_storage::MSC_CLASS && iface.sub == crate::usb_storage::MSC_SUBCLASS_SCSI && iface.proto == crate::usb_storage::MSC_PROTO_BBB {
                    let mut ep_in: u8 = 0;
                    let mut ep_out: u8 = 0;
                    let mut max_pkt: u16 = 512;
                    let mut eoff = off + iface.len as usize;
                    while eoff + 2 < total as usize {
                        let elen = *dbuf.add(eoff) as usize;
                        let etyp = *dbuf.add(eoff + 1);
                        if elen == 0 || eoff + elen > total as usize { break; }
                        if etyp == 5 {  // DT_ENDP
                            let ep = &*(dbuf.add(eoff) as *const EpDesc);
                            if ep.addr & 0x80 != 0 { ep_in = ep.addr; max_pkt = ep.maxsz; }
                            else { ep_out = ep.addr; }
                        }
                        eoff += elen;
                    }
                    if ep_in != 0 && ep_out != 0 {
                        crate::usb_storage::register_storage(addr, ep_in, ep_out, max_pkt);
                    }
                }
            }
            off += len;
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_ehci_init(mmio_phys: u64) -> i32 {
    let base = krust_map_mmio(mmio_phys, 256);
    if base == 0 { return -1; }
    if init_ehci(base as *mut u8) { 0 } else { -1 }
}

#[no_mangle]
pub unsafe extern "C" fn krust_ehci_poll() {
    let mmio = (OP - ptr::read_volatile((OP - CAPLENGTH) as *const u8) as u64) as *mut u8;
    let hcsp = ptr::read_volatile((mmio as u64 + HCSPARAMS) as *mut u32);
    let nports = hcsp & SP_NPORTS;
    for p in 0..nports { poll_port(p as u8); }
    for i in 0..NHID { poll_hid(&mut HIDS[i]); }
}

#[no_mangle]
pub unsafe extern "C" fn krust_ehci_get_event() -> UsbEvent {
    let z = UsbEvent { typ: 0, mods: 0, key: 0, dx: 0, dy: 0, btn: 0 };
    if EVT == EVH { return z; }
    let e = EVS[EVT]; EVT = (EVT + 1) & 15; e
}

#[no_mangle]
pub unsafe extern "C" fn krust_ehci_device_count() -> i32 { NDEV as i32 }
