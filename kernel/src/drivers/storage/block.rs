use core::fmt;

pub const MAX_BLOCK_DEVICES: usize = 16;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BlockDriverType {
    ATA,
    NVMe,
    AHCI,
    VirtIO,
    USB,
}

impl fmt::Display for BlockDriverType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BlockDriverType::ATA => write!(f, "ata"),
            BlockDriverType::NVMe => write!(f, "nvme"),
            BlockDriverType::AHCI => write!(f, "ahci"),
            BlockDriverType::VirtIO => write!(f, "virtio"),
            BlockDriverType::USB => write!(f, "usb"),
        }
    }
}

pub struct BlockDevice {
    pub name: [u8; 32],
    pub driver: BlockDriverType,
    pub drive_index: i32,
    pub sector_size: u32,
    pub total_sectors: u64,
    pub read_fn: Option<fn(i32, u64, u32, *mut u8) -> i32>,
    pub write_fn: Option<fn(i32, u64, u32, *const u8) -> i32>,
    pub flush_fn: Option<fn(i32) -> i32>,
}

unsafe impl Send for BlockDevice {}

const fn none_array<const N: usize>() -> [Option<BlockDevice>; N] {
    // SAFETY: Option<BlockDevice> is valid when all bytes are zero, and None is zero
    unsafe { core::mem::zeroed() }
}

static mut BLOCK_DEVICES: [Option<BlockDevice>; MAX_BLOCK_DEVICES] = none_array::<MAX_BLOCK_DEVICES>();
static mut BLOCK_DEVICE_COUNT: usize = 0;

fn ata_read_wrapper(drive: i32, lba: u64, count: u32, buf: *mut u8) -> i32 {
    unsafe {
        if crate::ata_pio::krust_ata_read(drive, lba as u32, count as u8, buf) {
            0
        } else {
            -1
        }
    }
}

fn ata_write_wrapper(drive: i32, lba: u64, count: u32, buf: *const u8) -> i32 {
    unsafe {
        if crate::ata_pio::krust_ata_write(drive, lba as u32, count as u8, buf) {
            0
        } else {
            -1
        }
    }
}

fn ata_flush_wrapper(drive: i32) -> i32 {
    unsafe { crate::ata_pio::krust_ata_flush(); }
    0
}

fn nvme_read_wrapper(drive: i32, lba: u64, count: u32, buf: *mut u8) -> i32 {
    unsafe { crate::nvme::nvme_read(lba, count, buf) }
}

fn nvme_write_wrapper(drive: i32, lba: u64, count: u32, buf: *const u8) -> i32 {
    unsafe { crate::nvme::nvme_write(lba, count, buf) }
}

fn nvme_flush_wrapper(_drive: i32) -> i32 {
    unsafe { crate::nvme::nvme_flush(); }
    0
}

fn ahci_read_wrapper(drive: i32, lba: u64, count: u32, buf: *mut u8) -> i32 {
    unsafe { crate::ahci::krust_ahci_read(drive, lba, count, buf) }
}

fn ahci_write_wrapper(drive: i32, lba: u64, count: u32, buf: *const u8) -> i32 {
    unsafe { crate::ahci::krust_ahci_write(drive, lba, count, buf) }
}

fn virtio_read_wrapper(drive: i32, lba: u64, count: u32, buf: *mut u8) -> i32 {
    unsafe { crate::virtio_blk::krust_virtio_blk_read(lba, buf, count) }
}

fn virtio_write_wrapper(drive: i32, lba: u64, count: u32, buf: *const u8) -> i32 {
    unsafe { crate::virtio_blk::krust_virtio_blk_write(lba, buf, count) }
}

fn usb_read_wrapper(drive: i32, lba: u64, count: u32, buf: *mut u8) -> i32 {
    unsafe { crate::usb_storage::usb_stor_read(drive as usize, lba, count, buf) }
}

fn usb_write_wrapper(drive: i32, lba: u64, count: u32, buf: *const u8) -> i32 {
    unsafe { crate::usb_storage::usb_stor_write(drive as usize, lba, count, buf) }
}

fn usb_flush_wrapper(_drive: i32) -> i32 { 0 }

pub fn register_block_device(
    name: &[u8],
    driver: BlockDriverType,
    drive_index: i32,
    sector_size: u32,
    total_sectors: u64,
    read_fn: Option<fn(i32, u64, u32, *mut u8) -> i32>,
    write_fn: Option<fn(i32, u64, u32, *const u8) -> i32>,
    flush_fn: Option<fn(i32) -> i32>,
) -> bool {
    unsafe {
        if BLOCK_DEVICE_COUNT >= MAX_BLOCK_DEVICES {
            return false;
        }
        let mut dev = BlockDevice {
            name: [0; 32],
            driver,
            drive_index,
            sector_size,
            total_sectors,
            read_fn,
            write_fn,
            flush_fn,
        };
        let copy_len = core::cmp::min(name.len(), 31);
        dev.name[..copy_len].copy_from_slice(&name[..copy_len]);
        BLOCK_DEVICES[BLOCK_DEVICE_COUNT] = Some(dev);
        BLOCK_DEVICE_COUNT += 1;
        true
    }
}

pub fn device_count() -> usize {
    unsafe { BLOCK_DEVICE_COUNT }
}

pub fn get_device(index: usize) -> Option<&'static mut BlockDevice> {
    unsafe {
        if index >= BLOCK_DEVICE_COUNT {
            return None;
        }
        BLOCK_DEVICES[index].as_mut()
    }
}

pub fn find_device(name: &[u8]) -> Option<&'static mut BlockDevice> {
    unsafe {
        for i in 0..BLOCK_DEVICE_COUNT {
            if let Some(ref mut dev) = BLOCK_DEVICES[i] {
                if dev.name.starts_with(name) {
                    return Some(dev);
                }
            }
        }
    }
    None
}

pub fn block_read(dev_index: usize, lba: u64, count: u32, buf: *mut u8) -> i32 {
    unsafe {
        if dev_index >= BLOCK_DEVICE_COUNT { return -1; }
        if let Some(ref dev) = BLOCK_DEVICES[dev_index] {
            if let Some(read_fn) = dev.read_fn {
                return read_fn(dev.drive_index, lba, count, buf);
            }
        }
    }
    -1
}

pub fn block_write(dev_index: usize, lba: u64, count: u32, buf: *const u8) -> i32 {
    unsafe {
        if dev_index >= BLOCK_DEVICE_COUNT { return -1; }
        if let Some(ref dev) = BLOCK_DEVICES[dev_index] {
            if let Some(write_fn) = dev.write_fn {
                return write_fn(dev.drive_index, lba, count, buf);
            }
        }
    }
    -1
}

pub fn block_flush(dev_index: usize) -> i32 {
    unsafe {
        if dev_index >= BLOCK_DEVICE_COUNT { return -1; }
        if let Some(ref dev) = BLOCK_DEVICES[dev_index] {
            if let Some(flush_fn) = dev.flush_fn {
                return flush_fn(dev.drive_index);
            }
        }
    }
    -1
}

pub fn detect_all_devices() {
    unsafe {
        let mut name_buf = [0u8; 32];

        if crate::ata_pio::krust_ata_drive_count() > 0 {
            for d in 0..crate::ata_pio::krust_ata_drive_count() {
                if crate::ata_pio::krust_ata_present(d) {
                    let name = b"sd";
                    name_buf[..2].copy_from_slice(name);
                    name_buf[2] = b'a' + d as u8;
                    name_buf[3] = 0;
                    let sectors = crate::ata_pio::krust_ata_get_total_sectors(d) as u64;
                    register_block_device(
                        &name_buf[..3],
                        BlockDriverType::ATA,
                        d,
                        512,
                        sectors,
                        Some(ata_read_wrapper),
                        Some(ata_write_wrapper),
                        Some(ata_flush_wrapper),
                    );
                }
            }
        }

        if crate::nvme::nvme_is_ready() {
            let name = b"nvme0n1";
            name_buf[..7].copy_from_slice(name);
            register_block_device(
                &name_buf[..7],
                BlockDriverType::NVMe,
                0,
                crate::nvme::nvme_block_size(),
                crate::nvme::nvme_block_count(),
                Some(nvme_read_wrapper),
                Some(nvme_write_wrapper),
                Some(nvme_flush_wrapper),
            );
        }

        if crate::ahci::krust_ahci_drive_count() > 0 {
            for d in 0..crate::ahci::krust_ahci_drive_count() {
                if crate::ahci::krust_ahci_present(d) != 0 {
                    let name = b"sd";
                    let letter_offset = 8u8 + d as u8;
                    name_buf[..2].copy_from_slice(name);
                    name_buf[2] = letter_offset;
                    name_buf[3] = 0;
                    let sectors = crate::ahci::krust_ahci_get_sectors(d);
                    register_block_device(
                        &name_buf[..3],
                        BlockDriverType::AHCI,
                        d,
                        512,
                        sectors,
                        Some(ahci_read_wrapper),
                        Some(ahci_write_wrapper),
                        None,
                    );
                }
            }
        }

        if crate::virtio_blk::krust_virtio_blk_capacity() > 0 {
            let name = b"vd";
            name_buf[..2].copy_from_slice(name);
            name_buf[2] = b'a';
            name_buf[3] = 0;
            let sectors = crate::virtio_blk::krust_virtio_blk_capacity();
            let bs = crate::virtio_blk::krust_virtio_blk_block_size();
            register_block_device(
                &name_buf[..3],
                BlockDriverType::VirtIO,
                0,
                bs,
                sectors,
                Some(virtio_read_wrapper),
                Some(virtio_write_wrapper),
                None,
            );
        }

        // USB Mass Storage devices
        let usb_count = crate::usb_storage::storage_device_count();
        for d in 0..usb_count {
            // Probe capacity
            if !crate::usb_storage::usb_stor_test_unit_ready(d) {
                continue;
            }
            if !crate::usb_storage::usb_stor_read_capacity(d) {
                continue;
            }
            if let Some(usb_dev) = crate::usb_storage::storage_device(d) {
                let sectors = usb_dev.block_count;
                let bs = usb_dev.block_size;
                if sectors == 0 { continue; }
                // Find next available letter for sdX
                // ATA uses 0..3, AHCI starts at 8, so USB devices get their own numbering
                let letter_offset = 16u8 + d as u8;
                name_buf[..2].copy_from_slice(b"sd");
                name_buf[2] = letter_offset;
                name_buf[3] = 0;
                register_block_device(
                    &name_buf[..3],
                    BlockDriverType::USB,
                    d as i32,
                    bs,
                    sectors,
                    Some(usb_read_wrapper),
                    Some(usb_write_wrapper),
                    Some(usb_flush_wrapper),
                );
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_block_read(dev: usize, lba: u64, count: u32, buf: *mut u8) -> i32 {
    block_read(dev, lba, count, buf)
}

#[no_mangle]
pub unsafe extern "C" fn krust_block_write(dev: usize, lba: u64, count: u32, buf: *const u8) -> i32 {
    block_write(dev, lba, count, buf)
}

#[no_mangle]
pub unsafe extern "C" fn krust_block_flush(dev: usize) -> i32 {
    block_flush(dev)
}

#[no_mangle]
pub unsafe extern "C" fn krust_block_device_count() -> u32 {
    BLOCK_DEVICE_COUNT as u32
}
