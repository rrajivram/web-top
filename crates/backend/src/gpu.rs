use shared::GpuCore;
use std::collections::HashSet;

/// Returns (gpu_engine_cores, set_of_pids_with_active_gpu_context).
pub fn collect_gpu_info() -> (Vec<GpuCore>, HashSet<u32>) {
    #[cfg(target_os = "macos")]
    return macos::collect();
    #[cfg(not(target_os = "macos"))]
    (vec![], HashSet::new())
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use core_foundation_sys::{
        base::{kCFAllocatorDefault, CFRelease, CFTypeRef},
        dictionary::{CFDictionaryGetValue, CFDictionaryRef, CFMutableDictionaryRef},
        number::{kCFNumberSInt32Type, CFNumberGetValue, CFNumberRef},
        string::{
            kCFStringEncodingUTF8, CFStringCreateWithCString, CFStringGetCString, CFStringRef,
        },
    };
    use io_kit_sys::{
        kIOMasterPortDefault, IOIteratorNext, IOObjectRelease,
        IORegistryEntryCreateCFProperties, IOServiceGetMatchingService, IOServiceMatching,
        types::{io_iterator_t, io_object_t, io_registry_entry_t},
    };
    use std::ffi::{c_char, c_void, CString};
    use std::ptr;

    // IOKit functions not re-exported by io-kit-sys
    extern "C" {
        fn IORegistryEntryGetChildIterator(
            entry: io_registry_entry_t,
            plane: *const c_char,
            iterator: *mut io_iterator_t,
        ) -> i32;
    }

    pub fn collect() -> (Vec<GpuCore>, HashSet<u32>) {
        unsafe {
            // IOServiceMatching returns a retained dict that GetMatchingService consumes
            let matching = IOServiceMatching(b"IOAccelerator\0".as_ptr() as *const c_char);
            if matching.is_null() {
                return (vec![], HashSet::new());
            }
            let service: io_object_t =
                IOServiceGetMatchingService(kIOMasterPortDefault, matching as _);
            if service == 0 {
                return (vec![], HashSet::new());
            }

            let cores = perf_stats(service);
            let pids = gpu_client_pids(service);
            IOObjectRelease(service);
            (cores, pids)
        }
    }

    unsafe fn perf_stats(service: io_registry_entry_t) -> Vec<GpuCore> {
        let mut props: CFMutableDictionaryRef = ptr::null_mut();
        if IORegistryEntryCreateCFProperties(service, &mut props, kCFAllocatorDefault, 0) != 0
            || props.is_null()
        {
            return vec![];
        }

        let perf_key = cf_str("PerformanceStatistics");
        let perf_dict =
            CFDictionaryGetValue(props as CFDictionaryRef, perf_key as *const c_void)
                as CFDictionaryRef;
        CFRelease(perf_key as CFTypeRef);

        let cores = if perf_dict.is_null() {
            vec![]
        } else {
            let device = dict_i32(perf_dict, "Device Utilization %").unwrap_or(0) as f32;
            let renderer = dict_i32(perf_dict, "Renderer Utilization %").unwrap_or(0) as f32;
            let tiler = dict_i32(perf_dict, "Tiler Utilization %").unwrap_or(0) as f32;
            vec![
                GpuCore { name: "Device".into(), usage: device },
                GpuCore { name: "Renderer".into(), usage: renderer },
                GpuCore { name: "Tiler".into(), usage: tiler },
                GpuCore { name: "ANE".into(), usage: -1.0 },
            ]
        };

        CFRelease(props as CFTypeRef);
        cores
    }

    unsafe fn gpu_client_pids(service: io_registry_entry_t) -> HashSet<u32> {
        let mut pids = HashSet::new();
        let mut iter: io_iterator_t = 0;
        let plane = b"IOService\0".as_ptr() as *const c_char;

        if IORegistryEntryGetChildIterator(service, plane, &mut iter) != 0 || iter == 0 {
            return pids;
        }

        loop {
            let child: io_object_t = IOIteratorNext(iter);
            if child == 0 {
                break;
            }
            if let Some(pid) = client_pid(child) {
                pids.insert(pid);
            }
            IOObjectRelease(child);
        }
        IOObjectRelease(iter);
        pids
    }

    // Read "IOUserClientCreator" = "pid NNN, processname" from a child entry.
    unsafe fn client_pid(entry: io_registry_entry_t) -> Option<u32> {
        let mut props: CFMutableDictionaryRef = ptr::null_mut();
        if IORegistryEntryCreateCFProperties(entry, &mut props, kCFAllocatorDefault, 0) != 0
            || props.is_null()
        {
            return None;
        }

        let key = cf_str("IOUserClientCreator");
        let val =
            CFDictionaryGetValue(props as CFDictionaryRef, key as *const c_void) as CFStringRef;
        CFRelease(key as CFTypeRef);

        let pid = if val.is_null() {
            None
        } else {
            let mut buf = [0u8; 256];
            let ok = CFStringGetCString(
                val,
                buf.as_mut_ptr() as *mut c_char,
                256,
                kCFStringEncodingUTF8,
            );
            if ok == 0 {
                None
            } else {
                // Format: "pid NNN, processname"
                let s = std::str::from_utf8(&buf)
                    .ok()
                    .and_then(|s| s.split('\0').next())
                    .unwrap_or("");
                s.strip_prefix("pid ")
                    .and_then(|s| s.split(',').next())
                    .and_then(|s| s.trim().parse().ok())
            }
        };

        CFRelease(props as CFTypeRef);
        pid
    }

    unsafe fn cf_str(s: &str) -> CFStringRef {
        let c = CString::new(s).unwrap();
        CFStringCreateWithCString(kCFAllocatorDefault, c.as_ptr(), kCFStringEncodingUTF8)
    }

    unsafe fn dict_i32(dict: CFDictionaryRef, key: &str) -> Option<i32> {
        let k = cf_str(key);
        if k.is_null() {
            return None;
        }
        let val = CFDictionaryGetValue(dict, k as *const c_void);
        CFRelease(k as CFTypeRef);
        if val.is_null() {
            return None;
        }
        let mut n: i32 = 0;
        let ok = CFNumberGetValue(
            val as CFNumberRef,
            kCFNumberSInt32Type,
            &mut n as *mut _ as *mut c_void,
        );
        if ok { Some(n) } else { None }
    }
}
