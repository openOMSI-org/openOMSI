//! The axes of wheels, pedals and joysticks on macOS, read from the HID elements
//! themselves. gilrs names an axis by its HID usage, so two axes of one usage became one:
//! the HORI Truck Control System declares its accelerator and brake as two Sliders (0x36)
//! back to back, and the brake pedal moved the accelerator. It also leaves out the
//! simulation page's steering and clutch. Here every axis element is its own axis, in the
//! order the device declares them, as DirectInput lists them on Windows (slider 0, 1).

use objc2_core_foundation::{CFArray, CFDictionary, CFNumber, CFRetained, CFSet, CFString};
use objc2_io_kit::{IOHIDDevice, IOHIDElement, IOHIDElementType, IOHIDManager, IOHIDValue};
use std::ptr::NonNull;

struct Axis {
    element: CFRetained<IOHIDElement>,
    code: u32,
    min: i64,
    max: i64,
    bits: u32,
}

struct Device {
    device: CFRetained<IOHIDDevice>,
    name: String,
    /// (vendor, product): gilrs lists a gamepad under its SDL mapping's name ("PS5
    /// Controller"), not the device's ("DualSense Wireless Controller"), so the two are
    /// paired by these
    id: Option<(u16, u16)>,
    axes: Vec<Axis>,
}

/// A device as last read: its name, (vendor, product) and axes (code, value -1..1) in
/// declared order.
pub struct HidAxes {
    pub name: String,
    pub id: Option<(u16, u16)>,
    pub axes: Vec<(u32, f32)>,
}

pub struct MacHid {
    manager: CFRetained<IOHIDManager>,
    devices: Vec<Device>,
    last_scan: Option<std::time::Instant>,
    count: isize,
}

// (the manager and the devices are only touched from the thread that polls the controllers)
unsafe impl Send for MacHid {}

fn string_property(device: &IOHIDDevice, key: &str) -> Option<String> {
    let v = device.property(&CFString::from_str(key))?;
    v.downcast::<CFString>().ok().map(|s| s.to_string())
}

fn number_property(device: &IOHIDDevice, key: &str) -> Option<i64> {
    let v = device.property(&CFString::from_str(key))?;
    v.downcast::<CFNumber>().ok().and_then(|n| n.as_i64())
}

fn is_axis(e: &IOHIDElement) -> bool {
    let t = e.r#type();
    if !(t == IOHIDElementType::Input_Misc || t == IOHIDElementType::Input_Axis) {
        return false;
    }
    match (e.usage_page(), e.usage()) {
        (1, 0x30..=0x38) => true,
        // simulation page: rudder, throttle, accelerator, brake, clutch, steering
        (2, 0xBA | 0xBB | 0xC4 | 0xC5 | 0xC6 | 0xC8) => true,
        _ => false,
    }
}

impl MacHid {
    pub fn new() -> Option<MacHid> {
        let manager = IOHIDManager::new(None, 0);
        // joysticks (wheels say they are one), gamepads and multi-axis controllers only: all
        // devices would take in the keyboard, which needs the Input Monitoring permission
        let matcher = |usage: i32| -> CFRetained<CFDictionary<CFString, CFNumber>> {
            let (pk, uk) = (
                CFString::from_static_str("DeviceUsagePage"),
                CFString::from_static_str("DeviceUsage"),
            );
            let (pv, uv) = (CFNumber::new_i32(1), CFNumber::new_i32(usage));
            CFDictionary::from_slices(&[&*pk, &*uk], &[&*pv, &*uv])
        };
        let matchers = CFArray::from_retained_objects(&[matcher(4), matcher(5), matcher(8)]);
        unsafe { manager.set_device_matching_multiple(Some(matchers.as_opaque())) };
        if manager.open(0) != 0 {
            log::warn!(
                "HID: cannot open the device manager; wheels and pedals are read through gilrs"
            );
            return None;
        }
        let mut h = MacHid {
            manager,
            devices: Vec::new(),
            last_scan: None,
            count: -1,
        };
        h.scan();
        Some(h)
    }

    /// Find the devices again when the set of them changed (checked every two seconds).
    fn scan(&mut self) {
        if self
            .last_scan
            .is_some_and(|t| t.elapsed().as_secs_f32() < 2.0)
        {
            return;
        }
        self.last_scan = Some(std::time::Instant::now());
        let Some(set) = self.manager.devices() else {
            self.devices.clear();
            self.count = 0;
            return;
        };
        let set: &CFSet = &set;
        let n = set.count();
        if n == self.count {
            return;
        }
        self.count = n;
        let mut raw: Vec<*const std::ffi::c_void> = vec![std::ptr::null(); n.max(0) as usize];
        unsafe { set.values(raw.as_mut_ptr()) };
        self.devices.clear();
        for p in raw {
            let Some(p) = NonNull::new(p as *mut IOHIDDevice) else {
                continue;
            };
            let device: CFRetained<IOHIDDevice> = unsafe { CFRetained::retain(p) };
            if !(device.conforms_to(1, 4) || device.conforms_to(1, 5) || device.conforms_to(1, 8)) {
                continue;
            }
            let Some(elements) = (unsafe { device.matching_elements(None, 0) }) else {
                continue;
            };
            let elements: CFRetained<CFArray<IOHIDElement>> =
                unsafe { CFRetained::cast_unchecked(elements) };
            let mut axes = Vec::new();
            let mut cookies = Vec::new();
            for e in elements.iter() {
                if !is_axis(&e) || cookies.contains(&e.cookie()) {
                    continue;
                }
                cookies.push(e.cookie());
                let bits = e.report_size().clamp(1, 32);
                let (mut min, mut max) = (e.logical_min() as i64, e.logical_max() as i64);
                // a 16-bit axis declared 0..65535 comes back from the descriptor as 0..-1
                // (the item is signed): the range is the report size's, unsigned
                if max <= min {
                    min = 0;
                    max = (1i64 << bits) - 1;
                }
                axes.push(Axis {
                    code: (e.usage_page() << 16) | e.usage(),
                    element: e,
                    min,
                    max,
                    bits,
                });
            }
            if axes.is_empty() {
                continue;
            }
            let name = string_property(&device, "Product").unwrap_or_else(|| "HID device".into());
            let id = number_property(&device, "VendorID")
                .zip(number_property(&device, "ProductID"))
                .map(|(v, p)| (v as u16, p as u16));
            log::info!(
                "HID: {name} {id:04x?}: {} axes {:?}",
                axes.len(),
                axes.iter()
                    .map(|a| format!("{:#x} {}..{}", a.code, a.min, a.max))
                    .collect::<Vec<_>>()
            );
            self.devices.push(Device {
                device,
                name,
                id,
                axes,
            });
        }
    }

    /// Every device with its axes.
    pub fn read(&mut self) -> Vec<HidAxes> {
        self.scan();
        let mut out = Vec::new();
        for d in &self.devices {
            let mut axes = Vec::with_capacity(d.axes.len());
            for a in &d.axes {
                let mut v: NonNull<IOHIDValue> = NonNull::dangling();
                let ok = unsafe { d.device.value(&a.element, NonNull::from(&mut v)) } == 0;
                let raw = if ok {
                    unsafe { v.as_ref() }.integer_value() as i64
                } else {
                    (a.min + a.max) / 2
                };
                // a value read back signed from an unsigned field
                let raw = if raw < a.min && a.min >= 0 {
                    raw + (1i64 << a.bits)
                } else {
                    raw
                };
                let span = (a.max - a.min).max(1) as f32;
                axes.push((
                    a.code,
                    (((raw - a.min) as f32 / span) * 2.0 - 1.0).clamp(-1.0, 1.0),
                ));
            }
            out.push(HidAxes {
                name: d.name.clone(),
                id: d.id,
                axes,
            });
        }
        out
    }
}
