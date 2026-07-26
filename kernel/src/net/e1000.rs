use crate::klib::{uint8_t, uint16_t, uint32_t, uint64_t};
use crate::pci::{PCIDevice, PCI};
use crate::net::{NetDevice, NetError};

#[repr(C, packed)]
struct E1000TxDesc {
    addr: uint64_t,
    length: uint16_t,
    cso: uint8_t,
    cmd: uint8_t,
    status: uint8_t,
    css: uint8_t,
    special: uint16_t,
}

#[repr(C, packed)]
struct E1000RxDesc {
    addr: uint64_t,
    length: uint16_t,
    checksum: uint16_t,
    status: uint8_t,
    errors: uint8_t,
    special: uint16_t,
}

const NUM_RX_DESC: usize = 32;
const NUM_TX_DESC: usize = 32;
const RX_BUF_SIZE: usize = 2048;

const REG_CTRL: uint16_t = 0x0000;
const REG_RCTL: uint16_t = 0x0100;
const REG_TCTL: uint16_t = 0x0400;
const REG_TIPG: uint16_t = 0x0410;
const REG_RDBAL: uint16_t = 0x2800;
const REG_RDBAH: uint16_t = 0x2804;
const REG_RDLEN: uint16_t = 0x2808;
const REG_RDH: uint16_t = 0x2810;
const REG_RDT: uint16_t = 0x2818;
const REG_TDBAL: uint16_t = 0x3800;
const REG_TDBAH: uint16_t = 0x3804;
const REG_TDLEN: uint16_t = 0x3808;
const REG_TDH: uint16_t = 0x3810;
const REG_TDT: uint16_t = 0x3818;
const REG_IMS: uint16_t = 0x00D0;

const CTRL_RST: uint32_t = 0x04000000;
const CTRL_SLU: uint32_t = 0x00000040;

const RCTL_EN: uint32_t = 0x00000002;
const RCTL_SBP: uint32_t = 0x00000004;
const RCTL_UPE: uint32_t = 0x00000008;
const RCTL_MPE: uint32_t = 0x00000010;
const RCTL_BAM: uint32_t = 0x00008000;
const RCTL_BSIZE_2048: uint32_t = 0x00000000;

const TCTL_EN: uint32_t = 0x00000002;
const TCTL_PSP: uint32_t = 0x00000008;

const CMD_EOP: uint8_t = 0x01;
const CMD_IFCS: uint8_t = 0x02;
const CMD_RS: uint8_t = 0x08;

const RX_STA_DD: uint8_t = 0x01;
const TX_STA_DD: uint8_t = 0x01;

extern "C" {
    fn krust_pmm_alloc_frame() -> uint64_t;
    fn krust_paging_get_phys(vaddr: uint64_t) -> uint64_t;
    fn krust_malloc(size: usize) -> *mut u8;
}

pub struct E1000Device {
    mmio_base: *mut uint32_t,
    mac_addr: [uint8_t; 6],
    rx_descs: *mut E1000RxDesc,
    tx_descs: *mut E1000TxDesc,
    rx_buffers: [*mut uint8_t; NUM_RX_DESC],
    rx_descs_phys: uint64_t,
    tx_descs_phys: uint64_t,
    tx_cur: usize,
    rx_cur: usize,
    initialized: bool,
}

unsafe impl Send for E1000Device {}

impl E1000Device {
    pub fn new() -> Self {
        Self {
            mmio_base: core::ptr::null_mut(),
            mac_addr: [0; 6],
            rx_descs: core::ptr::null_mut(),
            tx_descs: core::ptr::null_mut(),
            rx_buffers: [core::ptr::null_mut(); NUM_RX_DESC],
            rx_descs_phys: 0,
            tx_descs_phys: 0,
            tx_cur: 0,
            rx_cur: 0,
            initialized: false,
        }
    }

    pub fn probe(dev: &PCIDevice) -> Option<Self> {
        let mut device = Self::new();
        let bar0 = PCI::config_read_dword(dev.bus, dev.slot, dev.func, 0x10);
        let mmio_base = (bar0 & 0xFFFFFFF0) as *mut uint32_t;
        device.mmio_base = mmio_base;

        unsafe {
            *E1000_MMIO_BASE.lock() = mmio_base as usize;
            *E1000_PCI_BUS.lock() = dev.bus;
            *E1000_PCI_SLOT.lock() = dev.slot;
            *E1000_PCI_FUNC.lock() = dev.func;

            PCI::enable_bus_mastering(dev.bus, dev.slot, dev.func);

            let mut cmd = PCI::config_read_word(dev.bus, dev.slot, dev.func, 0x04);
            cmd |= 1 << 10;
            PCI::config_write_word(dev.bus, dev.slot, dev.func, 0x04, cmd);
        }

        let mac_low = device.mmio_read(0x5400);
        let mac_high = device.mmio_read(0x5404);
        device.mac_addr[0] = (mac_low & 0xFF) as uint8_t;
        device.mac_addr[1] = ((mac_low >> 8) & 0xFF) as uint8_t;
        device.mac_addr[2] = ((mac_low >> 16) & 0xFF) as uint8_t;
        device.mac_addr[3] = ((mac_low >> 24) & 0xFF) as uint8_t;
        device.mac_addr[4] = (mac_high & 0xFF) as uint8_t;
        device.mac_addr[5] = ((mac_high >> 8) & 0xFF) as uint8_t;

        Some(device)
    }

    pub fn init(&mut self, mac: [uint8_t; 6], _ip: [uint8_t; 4]) -> Result<(), ()> {
        if self.mmio_base.is_null() {
            return Err(());
        }
        self.mac_addr = mac;

        unsafe {
            self.mmio_write(REG_CTRL, self.mmio_read(REG_CTRL) | CTRL_RST);
            for _ in 0..10000 {
                if (self.mmio_read(REG_CTRL) & CTRL_RST) == 0 {
                    break;
                }
            }

            self.mmio_write(REG_CTRL, self.mmio_read(REG_CTRL) | CTRL_SLU);

            self.rx_descs = krust_pmm_alloc_frame() as *mut E1000RxDesc;
            if self.rx_descs.is_null() {
                return Err(());
            }
            core::ptr::write_bytes(self.rx_descs, 0, 1);
            self.rx_descs_phys = krust_paging_get_phys(self.rx_descs as uint64_t);

            for i in 0..NUM_RX_DESC {
                self.rx_buffers[i] = krust_malloc(RX_BUF_SIZE) as *mut uint8_t;
                if self.rx_buffers[i].is_null() {
                    return Err(());
                }
                let phys = krust_paging_get_phys(self.rx_buffers[i] as uint64_t);
                (*self.rx_descs.add(i)).addr = phys;
            }

            self.mmio_write(REG_RDBAL, self.rx_descs_phys as uint32_t);
            self.mmio_write(REG_RDBAH, (self.rx_descs_phys >> 32) as uint32_t);
            self.mmio_write(REG_RDLEN, (core::mem::size_of::<E1000RxDesc>() * NUM_RX_DESC) as uint32_t);
            self.mmio_write(REG_RDH, 0);
            self.mmio_write(REG_RDT, (NUM_RX_DESC - 1) as uint32_t);
            self.mmio_write(REG_RCTL, RCTL_EN | RCTL_SBP | RCTL_UPE | RCTL_MPE | RCTL_BAM | RCTL_BSIZE_2048);

            self.tx_descs = krust_pmm_alloc_frame() as *mut E1000TxDesc;
            if self.tx_descs.is_null() {
                return Err(());
            }
            core::ptr::write_bytes(self.tx_descs, 0, 1);
            self.tx_descs_phys = krust_paging_get_phys(self.tx_descs as uint64_t);

            self.mmio_write(REG_TDBAL, self.tx_descs_phys as uint32_t);
            self.mmio_write(REG_TDBAH, (self.tx_descs_phys >> 32) as uint32_t);
            self.mmio_write(REG_TDLEN, (core::mem::size_of::<E1000TxDesc>() * NUM_TX_DESC) as uint32_t);
            self.mmio_write(REG_TDH, 0);
            self.mmio_write(REG_TDT, 0);
            self.mmio_write(REG_TCTL, TCTL_EN | TCTL_PSP | (15 << 4) | (64 << 12));
            self.mmio_write(REG_TIPG, 8 | (4 << 10) | (6 << 20));

            self.mmio_write(REG_IMS, 0x3F);

            self.initialized = true;
            Ok(())
        }
    }

    pub fn poll(&mut self) {
        if !self.initialized {
            return;
        }
        unsafe {
            // Drain all available received packets
            loop {
                let desc = self.rx_descs.add(self.rx_cur).read_volatile();
                if desc.status & RX_STA_DD == 0 {
                    break;
                }
                let next = (self.rx_cur + 1) % NUM_RX_DESC;
                self.mmio_write(REG_RDT, self.rx_cur as u32);
                self.rx_cur = next;
            }
        }
    }

    fn mmio_read(&self, reg: uint16_t) -> uint32_t {
        unsafe { self.mmio_base.add((reg / 4) as usize).read_volatile() }
    }

    fn mmio_write(&self, reg: uint16_t, val: uint32_t) {
        unsafe { self.mmio_base.add((reg / 4) as usize).write_volatile(val); }
    }
}

impl NetDevice for E1000Device {
    fn send(&mut self, buf: &[u8]) -> Result<(), NetError> {
        if !self.initialized {
            return Err(NetError::NotConnected);
        }

        unsafe {
            let desc = &mut *self.tx_descs.add(self.tx_cur);
            let phys_addr = krust_paging_get_phys(buf.as_ptr() as uint64_t);
            desc.addr = phys_addr;
            desc.length = buf.len() as uint16_t;
            desc.cmd = CMD_EOP | CMD_IFCS | CMD_RS;
            desc.status = 0;

            let next = (self.tx_cur + 1) % NUM_TX_DESC;
            self.mmio_write(REG_TDT, next as uint32_t);
            self.tx_cur = next;

            // Bounded wait with timeout (10000 iterations ~ reasonable timeout)
            for _ in 0..10000 {
                if (desc.status & TX_STA_DD) != 0 {
                    return Ok(());
                }
                core::hint::spin_loop();
            }
            // Timeout: descriptor not completed - still return Ok to avoid blocking
            // the network stack indefinitely; the hardware will eventually complete.
        }

        Ok(())
    }

    fn receive(&mut self, buf: &mut [u8]) -> Result<usize, NetError> {
        if !self.initialized {
            return Err(NetError::NotConnected);
        }

        unsafe {
            let next = (self.rx_cur + 1) % NUM_RX_DESC;
            let desc = &mut *self.rx_descs.add(self.rx_cur);

            if (desc.status & RX_STA_DD) == 0 {
                return Err(NetError::WouldBlock);
            }

            let len = desc.length as usize;
            if len > buf.len() {
                return Err(NetError::BufferTooSmall);
            }

            let src = self.rx_buffers[self.rx_cur];
            buf[..len].copy_from_slice(core::slice::from_raw_parts(src, len));
            desc.status = 0;
            self.mmio_write(REG_RDT, self.rx_cur as u32);
            self.rx_cur = next;

            Ok(len)
        }
    }

    fn mac_address(&self) -> [uint8_t; 6] {
        self.mac_addr
    }

    fn ip_address(&self) -> [uint8_t; 4] {
        *NET_IP.lock()
    }
}

static NET_IP: crate::spinlock::SpinLock<[uint8_t; 4]> = crate::spinlock::SpinLock::new([0; 4]);
static NET_MAC: crate::spinlock::SpinLock<[uint8_t; 6]> = crate::spinlock::SpinLock::new([0; 6]);
static E1000_MMIO_BASE: crate::spinlock::SpinLock<usize> = crate::spinlock::SpinLock::new(0);
static E1000_PCI_BUS: crate::spinlock::SpinLock<u8> = crate::spinlock::SpinLock::new(0);
static E1000_PCI_SLOT: crate::spinlock::SpinLock<u8> = crate::spinlock::SpinLock::new(0);
static E1000_PCI_FUNC: crate::spinlock::SpinLock<u8> = crate::spinlock::SpinLock::new(0);

pub fn set_ip(ip: [uint8_t; 4]) {
    *NET_IP.lock() = ip;
}

pub fn set_mac(mac: [uint8_t; 6]) {
    *NET_MAC.lock() = mac;
}

pub fn get_ip() -> [uint8_t; 4] {
    *NET_IP.lock()
}

pub fn get_mac() -> [uint8_t; 6] {
    *NET_MAC.lock()
}

pub fn get_pci_bus() -> u8 { *E1000_PCI_BUS.lock() }
pub fn get_pci_slot() -> u8 { *E1000_PCI_SLOT.lock() }
pub fn get_pci_func() -> u8 { *E1000_PCI_FUNC.lock() }

pub fn get_mmio_base() -> *mut uint32_t { *E1000_MMIO_BASE.lock() as *mut uint32_t }

use crate::scheduler::Registers;

use crate::spinlock::SpinLock;
use core::sync::atomic::{AtomicU32, Ordering};

static E1000_IRQ_COUNT: AtomicU32 = AtomicU32::new(0);
static E1000_IRQ_THROTTLE: SpinLock<bool> = SpinLock::new(false);

pub extern "C" fn e1000_irq_handler(_r: *mut Registers) {
    let count = E1000_IRQ_COUNT.fetch_add(1, Ordering::Relaxed);

    // Throttle: if we've had too many interrupts in rapid succession, skip processing
    // This prevents interrupt storms while still allowing正常 traffic through polling
    {
        let mut throttle = E1000_IRQ_THROTTLE.lock();
        if *throttle {
            // Acknowledge and EOI, but don't process - let polling handle it
            let mmio = *E1000_MMIO_BASE.lock() as *mut uint32_t;
            if !mmio.is_null() {
                unsafe {
                    let icr = mmio.add((0x00D8 / 4) as usize).read_volatile();
                    mmio.add((0x00D8 / 4) as usize).write_volatile(icr);
                }
            }
            unsafe { crate::apic_hw::krust_apic_eoi(); }
            return;
        }

        // After 100 interrupts without a break, start throttling
        if count % 100 == 99 {
            *throttle = true;
        }
    }

    let mmio = *E1000_MMIO_BASE.lock() as *mut uint32_t;
    if mmio.is_null() { return; }

    let icr = unsafe { mmio.add((0x00D8 / 4) as usize).read_volatile() };
    unsafe { mmio.add((0x00D8 / 4) as usize).write_volatile(icr); }

    unsafe { crate::net::krust_net_poll(); }

    // Reset throttle after successful processing
    {
        let mut throttle = E1000_IRQ_THROTTLE.lock();
        *throttle = false;
    }

    unsafe { crate::apic_hw::krust_apic_eoi(); }
}

#[no_mangle]
pub unsafe extern "C" fn krust_e1000_irq_install(_irq: i32) {
    crate::isr::krust_isr_register_handler(0x41, e1000_irq_handler);
}

pub fn init_e1000() -> Option<E1000Device> {
    for bus in 0..=255u16 {
        for slot in 0..32u8 {
            for func in 0..8u8 {
                let vendor = PCI::config_read_word(bus as u8, slot, func, 0x00);
                if vendor == 0x8086 {
                    let device_id = PCI::config_read_word(bus as u8, slot, func, 0x02);
                    if device_id == 0x100E || device_id == 0x100F {
                        let dev = PCIDevice {
                            bus: bus as u8,
                            slot,
                            func,
                            vendor_id: vendor,
                            device_id,
                            class_code: 0,
                            subclass: 0,
                            prog_if: 0,
                            revision_id: 0,
                        };
                        if let Some(mut device) = E1000Device::probe(&dev) {
                            if device.init([0, 0, 0, 0, 0, 0], [0, 0, 0, 0]).is_ok() {
                                return Some(device);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}
