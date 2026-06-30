// SPDX-License-Identifier: GPL-2.0-only
#![allow(missing_docs)]
#![allow(unused_attributes)]
#![no_std]

use core::ffi::c_void;
use kernel::alloc::{allocator::Kmalloc, flags, Box};
use kernel::error::{code::EIO, code::ENODEV, code::ENOMEM};
use kernel::prelude::*;

use kernel::bindings::{
    device, i2c_adapter, i2c_board_info, i2c_client, i2c_device_id, i2c_driver, i2c_msg,
};

const DDC_ADDR: u16 = 0x37;
const DDC_HOST_SOURCE: u8 = 0x51;
const VCP_BRIGHTNESS: u8 = 0x10;
const MAX_MAPPINGS: usize = 32;

#[repr(C)]
pub struct backlight_device {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct backlight_properties {
    pub brightness: i32,
    pub max_brightness: i32,
    pub power: i32,
    pub type_: u32,
    pub fb_blank: i32,
}

#[repr(C)]
#[derive(Default)]
pub struct backlight_ops {
    pub options: u32,
    pub update_status: Option<unsafe extern "C" fn(bd: *mut backlight_device) -> i32>,
    pub get_brightness: Option<unsafe extern "C" fn(bd: *mut backlight_device) -> i32>,
}

#[allow(improper_ctypes)]
extern "C" {
    fn backlight_device_register(
        name: *const u8,
        dev: *mut device,
        data: *mut c_void,
        ops: *const backlight_ops,
        props: *const backlight_properties,
    ) -> *mut backlight_device;

    fn backlight_device_unregister(bd: *mut backlight_device);
    fn i2c_transfer(adap: *mut i2c_adapter, msgs: *mut i2c_msg, num: i32) -> i32;
    fn i2c_register_driver(owner: *mut c_void, driver: *mut i2c_driver) -> i32;
    fn i2c_del_driver(driver: *mut i2c_driver);
    fn i2c_for_each_dev(
        data: *mut c_void,
        fn_ptr: Option<unsafe extern "C" fn(d: *mut c_void, data: *mut c_void) -> i32>,
    ) -> i32;
    fn i2c_verify_adapter(dev: *mut c_void) -> *mut i2c_adapter;
    fn i2c_new_client_device(
        adap: *mut i2c_adapter,
        info: *const i2c_board_info,
    ) -> *mut i2c_client;
    fn i2c_unregister_device(client: *mut i2c_client);
    fn msleep(ms: u32);
}

#[derive(Copy, Clone)]
struct DeviceMapping {
    bl_dev: *mut backlight_device,
    client: *mut i2c_client,
    props: *mut backlight_properties,
}

struct DriverRegistry {
    mappings: [DeviceMapping; MAX_MAPPINGS],
    count: usize,
}

unsafe impl Send for DriverRegistry {}
unsafe impl Sync for DriverRegistry {}

static mut RAW_REGISTRY: DriverRegistry = DriverRegistry {
    mappings: [DeviceMapping {
        bl_dev: core::ptr::null_mut(),
        client: core::ptr::null_mut(),
        props: core::ptr::null_mut(),
    }; MAX_MAPPINGS],
    count: 0,
};

struct SliceWriter<'a> {
    slice: &'a mut [u8],
    cursor: usize,
}

impl<'a> core::fmt::Write for SliceWriter<'a> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        let rem = self.slice.len() - self.cursor;
        if bytes.len() > rem {
            return Err(core::fmt::Error);
        }
        self.slice[self.cursor..self.cursor + bytes.len()].copy_from_slice(bytes);
        self.cursor += bytes.len();
        Ok(())
    }
}

unsafe fn log_bytes(prefix: &str, bus_nr: i32, buf: &[u8], len: usize) {
    let mut hex_buf = [0u8; 64];
    let cursor = {
        let mut writer = SliceWriter {
            slice: &mut hex_buf,
            cursor: 0,
        };
        for i in 0..len {
            if i >= buf.len() {
                break;
            }
            let byte = buf[i];
            let high = byte >> 4;
            let low = byte & 0x0F;

            let c_high = if high < 10 {
                b'0' + high
            } else {
                b'a' + (high - 10)
            };
            let c_low = if low < 10 {
                b'0' + low
            } else {
                b'a' + (low - 10)
            };

            let _ = core::fmt::write(
                &mut writer,
                format_args!("{}{}", c_high as char, c_low as char),
            );
            let _ = core::fmt::write(&mut writer, format_args!(" "));
        }
        writer.cursor
    };

    if cursor < hex_buf.len() {
        hex_buf[cursor] = 0;
        unsafe {
            pr_info!(
                "ddcci_backlight: [Bus {}] {} {}\n",
                bus_nr,
                prefix,
                core::str::from_utf8_unchecked(&hex_buf[..cursor])
            );
        }
    }
}

unsafe extern "C" fn ddcci_update_status(bd: *mut backlight_device) -> i32 {
    let mut target_mapping = None;
    unsafe {
        let registry = &raw mut RAW_REGISTRY;
        for i in 0..(*registry).count {
            if (*registry).mappings[i].bl_dev == bd {
                target_mapping = Some((*registry).mappings[i]);
                break;
            }
        }
    }

    let map = match target_mapping {
        Some(m) => m,
        None => return -ENODEV.to_errno(),
    };

    unsafe {
        let client = map.client;
        if client.is_null() {
            return -ENODEV.to_errno();
        }

        let adap = (*client).adapter;
        if adap.is_null() {
            return -ENODEV.to_errno();
        }

        let bus_nr = (*adap).nr;

        // CRITICAL FIX: Cast the live kernel backlight device pointer
        // directly to read the true runtime sysfs property state frame!
        let live_props = bd as *mut backlight_properties;
        let target_brightness = (*live_props).brightness as u16;

        let mut pkt = [0u8; 7];
        pkt[0] = DDC_HOST_SOURCE;
        pkt[1] = 0x04;
        pkt[2] = 0x03;
        pkt[3] = VCP_BRIGHTNESS;
        pkt[4] = (target_brightness >> 8) as u8;
        pkt[5] = (target_brightness & 0xFF) as u8;

        let mut sum: u16 = 0x6E;
        for i in 0..6 {
            sum = sum.wrapping_add(pkt[i] as u16);
        }
        pkt[6] = (0u16.wrapping_sub(sum) & 0xFF) as u8;

        log_bytes("DDC SET_VCP TX:", bus_nr, &pkt, 7);

        let mut msg = i2c_msg {
            addr: DDC_ADDR,
            flags: 0,
            len: 7,
            buf: pkt.as_mut_ptr(),
        };
        let ret = i2c_transfer(adap, &mut msg, 1);
        if ret != 1 {
            pr_err!(
                "ddcci_backlight: [Bus {}] I2C write transaction failed: returning status {}\n",
                bus_nr,
                ret
            );
            return -EIO.to_errno();
        }

        msleep(50);
    }
    0
}
unsafe extern "C" fn ddcci_get_brightness(bd: *mut backlight_device) -> i32 {
    let mut target_mapping = None;
    unsafe {
        let registry = &raw mut RAW_REGISTRY;
        for i in 0..(*registry).count {
            if (*registry).mappings[i].bl_dev == bd {
                target_mapping = Some((*registry).mappings[i]);
                break;
            }
        }
    }

    let map = match target_mapping {
        Some(m) => m,
        None => return -ENODEV.to_errno(),
    };

    unsafe {
        let client = map.client;
        let props = map.props;
        if client.is_null() || props.is_null() {
            return -ENODEV.to_errno();
        }

        let adap = (*client).adapter;
        if adap.is_null() {
            return -ENODEV.to_errno();
        }

        let bus_nr = (*adap).nr;

        let mut req = [0u8; 5];
        req[0] = DDC_HOST_SOURCE;
        req[1] = 0x82;
        req[2] = 0x01;
        req[3] = VCP_BRIGHTNESS;

        let mut sum: u16 = 0x6E;
        for i in 0..4 {
            sum = sum.wrapping_add(req[i] as u16);
        }
        req[4] = (0u16.wrapping_sub(sum) & 0xFF) as u8;

        for attempt in 0..3 {
            log_bytes("DDC GET_VCP TX:", bus_nr, &req, 5);

            let mut req_msg = i2c_msg {
                addr: DDC_ADDR,
                flags: 0,
                len: 5,
                buf: req.as_mut_ptr(),
            };
            let write_ret = i2c_transfer(adap, &mut req_msg, 1);
            if write_ret != 1 {
                pr_warn!(
                    "ddcci_backlight: [Bus {}] TX attempt {} failed with {}\n",
                    bus_nr,
                    attempt,
                    write_ret
                );
                msleep(50);
                continue;
            }

            msleep(60);

            let mut resp = [0u8; 12];
            let mut resp_msg = i2c_msg {
                addr: DDC_ADDR,
                flags: 1,
                len: resp.len() as u16,
                buf: resp.as_mut_ptr(),
            };
            let read_ret = i2c_transfer(adap, &mut resp_msg, 1);

            if read_ret == 1 {
                log_bytes("DDC GET_VCP RX:", bus_nr, &resp, 12);

                let base = if resp[0] == 0x02 {
                    Some(0)
                } else if resp[0] == 0x6E && resp[2] == 0x02 {
                    Some(2)
                } else {
                    None
                };

                if let Some(b) = base {
                    if b + 2 < resp.len() && resp[b + 2] == VCP_BRIGHTNESS {
                        let max_hi = resp[b + 4] as i32;
                        let max_lo = resp[b + 5] as i32;
                        let max_val = (max_hi << 8) | max_lo;

                        let cur_hi = resp[b + 6] as i32;
                        let cur_lo = resp[b + 7] as i32;
                        let cur_val = (cur_hi << 8) | cur_lo;

                        pr_info!(
                            "ddcci_backlight: [Bus {}] Parsed successfully! Current: {}, Max: {}\n",
                            bus_nr,
                            cur_val,
                            max_val
                        );

                        if max_val > 0 {
                            (*props).max_brightness = max_val;
                        }
                        (*props).brightness = cur_val;
                        return cur_val;
                    } else {
                        pr_warn!("ddcci_backlight: [Bus {}] Opcode mismatch or out of bounds at base offset {}\n", bus_nr, b);
                    }
                } else {
                    pr_warn!(
                        "ddcci_backlight: [Bus {}] Invalid protocol header signature: 0x{:02x}\n",
                        bus_nr,
                        resp[0]
                    );
                }
            } else {
                pr_warn!(
                    "ddcci_backlight: [Bus {}] RX attempt {} read failed with {}\n",
                    bus_nr,
                    attempt,
                    read_ret
                );
            }
            msleep(50);
        }
        (*props).brightness
    }
}

// --- SUBSYSTEM PROBE ---

static mut BACKLIGHT_OPS: backlight_ops = backlight_ops {
    options: 0,
    update_status: Some(ddcci_update_status),
    get_brightness: Some(ddcci_get_brightness),
};

unsafe extern "C" fn ddcci_probe(client: *mut i2c_client) -> i32 {
    if client.is_null() {
        return -ENODEV.to_errno();
    }
    unsafe {
        let adap = (*client).adapter;
        if adap.is_null() {
            return -ENODEV.to_errno();
        }

        let mut probe_req = [DDC_HOST_SOURCE, 0x81, 0x01, 0x00];
        let mut sum: u16 = 0x6E;
        for i in 0..3 {
            sum = sum.wrapping_add(probe_req[i] as u16);
        }
        probe_req[3] = (0u16.wrapping_sub(sum) & 0xFF) as u8;

        let mut probe_msg = i2c_msg {
            addr: (*client).addr,
            flags: 0,
            len: 4,
            buf: probe_req.as_mut_ptr(),
        };

        if i2c_transfer(adap, &mut probe_msg, 1) != 1 {
            return -ENODEV.to_errno();
        }

        let bus_nr = (*adap).nr;
        let parent_device_ptr = if (*client).dev.parent.is_null() {
            &mut (*client).dev as *mut device
        } else {
            (*client).dev.parent
        };

        let mut name_buf = [0u8; 32];
        {
            let mut writer = SliceWriter {
                slice: &mut name_buf,
                cursor: 0,
            };
            let _ = core::fmt::write(&mut writer, format_args!("ddcci_bl_bus{}\0", bus_nr));
        }

        // Standardized directly to 0-100 to map perfectly across the display frame bounds
        let props_box = match Box::<backlight_properties, Kmalloc>::new(
            backlight_properties {
                brightness: 50,
                max_brightness: 100,
                power: 0,
                type_: 1,
                fb_blank: 0,
            },
            flags::GFP_KERNEL,
        ) {
            Ok(b) => Box::into_raw(b),
            Err(_) => return -ENOMEM.to_errno(),
        };

        let bd = backlight_device_register(
            name_buf.as_ptr(),
            parent_device_ptr,
            core::ptr::null_mut(),
            &raw mut BACKLIGHT_OPS,
            props_box,
        );

        if bd.is_null() {
            let _ = Box::<backlight_properties, Kmalloc>::from_raw(props_box);
            return -ENODEV.to_errno();
        }

        let registry = &raw mut RAW_REGISTRY;
        let idx = (*registry).count;
        if idx < MAX_MAPPINGS {
            (*registry).mappings[idx] = DeviceMapping {
                bl_dev: bd,
                client,
                props: props_box,
            };
            (*registry).count += 1;
        }

        let _ = ddcci_get_brightness(bd);

        pr_info!(
            "ddcci_backlight: Registered backlight device /sys/class/backlight/ddcci_bl_bus{}\n",
            bus_nr
        );
        0
    }
}

unsafe extern "C" fn ddcci_remove(client: *mut i2c_client) {
    if client.is_null() {
        return;
    }
    unsafe {
        let registry = &raw mut RAW_REGISTRY;
        for i in 0..(*registry).count {
            if (*registry).mappings[i].client == client {
                if !(*registry).mappings[i].bl_dev.is_null() {
                    backlight_device_unregister((*registry).mappings[i].bl_dev);
                }
                if !(*registry).mappings[i].props.is_null() {
                    let _ = Box::<backlight_properties, Kmalloc>::from_raw(
                        (*registry).mappings[i].props,
                    );
                }
                (*registry).mappings[i] = DeviceMapping {
                    bl_dev: core::ptr::null_mut(),
                    client: core::ptr::null_mut(),
                    props: core::ptr::null_mut(),
                };
                break;
            }
        }
    }
}

struct AdapterScanner {
    instantiated_clients: [*mut i2c_client; MAX_MAPPINGS],
    client_count: usize,
}

unsafe extern "C" fn scan_and_instantiate_ddc_devices(
    dev_ptr: *mut c_void,
    data_ptr: *mut c_void,
) -> i32 {
    unsafe {
        if dev_ptr.is_null() || data_ptr.is_null() {
            return 0;
        }
        let adapter = i2c_verify_adapter(dev_ptr);
        if adapter.is_null() {
            return 0;
        }

        let scanner = &mut *(data_ptr as *mut AdapterScanner);
        let i2c_class_ddc = 1 << 0;

        if (((*adapter).class & i2c_class_ddc) != 0 || (*adapter).nr == 12)
            && scanner.client_count < MAX_MAPPINGS
        {
            let mut info: i2c_board_info = core::mem::zeroed();
            let name_bytes = b"ddcci_backlight\0";
            core::ptr::copy_nonoverlapping(
                name_bytes.as_ptr() as *const kernel::ffi::c_char,
                info.type_.as_mut_ptr(),
                name_bytes.len(),
            );
            info.addr = DDC_ADDR;

            let client = i2c_new_client_device(adapter, &info);
            if !client.is_null() {
                scanner.instantiated_clients[scanner.client_count] = client;
                scanner.client_count += 1;
            }
        }
    }
    0
}

const fn make_i2c_device_id_name(src: &[u8]) -> [kernel::ffi::c_char; 20] {
    let mut arr = [0; 20];
    let mut i = 0;
    while i < src.len() && i < 20 {
        arr[i] = src[i] as kernel::ffi::c_char;
        i += 1;
    }
    arr
}

static ID_TABLE: [i2c_device_id; 2] = [
    i2c_device_id {
        name: make_i2c_device_id_name(b"ddcci_backlight"),
        driver_data: 0,
    },
    i2c_device_id {
        name: [0; 20],
        driver_data: 0,
    },
];

static mut DRIVER_MODEL: i2c_driver = unsafe { core::mem::zeroed() };
static mut ACTIVE_SCANNER_CONTEXT: AdapterScanner = AdapterScanner {
    instantiated_clients: [core::ptr::null_mut(); MAX_MAPPINGS],
    client_count: 0,
};

pub struct DdcciBacklight;

module! {
    type: DdcciBacklight,
    name: "ddcci_backlight",
    description: "Autodiscovery Rust DDC/CI Backlight Driver",
    license: "GPL",
}

impl kernel::Module for DdcciBacklight {
    fn init(module: &'static ThisModule) -> Result<Self> {
        pr_info!("ddcci_backlight: Initializing driver module\n");
        unsafe {
            DRIVER_MODEL.driver.name = b"ddcci_backlight\0".as_ptr() as *const kernel::ffi::c_char;
            DRIVER_MODEL.probe = Some(ddcci_probe);
            DRIVER_MODEL.remove = Some(ddcci_remove);
            DRIVER_MODEL.id_table = &ID_TABLE as *const i2c_device_id;

            let ret = i2c_register_driver(module.as_ptr() as *mut c_void, &raw mut DRIVER_MODEL);
            if ret < 0 {
                return Err(kernel::error::code::ENODEV);
            }

            i2c_for_each_dev(
                &raw mut ACTIVE_SCANNER_CONTEXT as *mut c_void,
                Some(scan_and_instantiate_ddc_devices),
            );
        }
        Ok(DdcciBacklight)
    }
}

impl Drop for DdcciBacklight {
    fn drop(&mut self) {
        unsafe {
            i2c_del_driver(&raw mut DRIVER_MODEL);
            for i in 0..ACTIVE_SCANNER_CONTEXT.client_count {
                let client = ACTIVE_SCANNER_CONTEXT.instantiated_clients[i];
                if !client.is_null() {
                    i2c_unregister_device(client);
                }
            }
        }
        pr_info!("ddcci_backlight: Module unloaded smoothly\n");
    }
}
