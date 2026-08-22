//! Asking DRM drivers about the hardware they drive.
//!
//! Whether a GPU is integrated cannot be told from its PCI class code. That
//! code says whether the GPU is the boot VGA device, not where it lives: an
//! AMD APU enumerates as `0x030000` (VGA compatible controller) when its iGPU
//! is the boot VGA device and as `0x038000` (display controller) when it is
//! not. The drivers themselves do know, and each has a query for it.
//!
//! Adding a driver means writing one query function and one match arm below.

use std::fs::{self, File, OpenOptions};
use std::mem::size_of;
use std::os::fd::AsRawFd;

/// Returns whether the driver considers the GPU at the given path in
/// `/sys/class/drm` to be integrated, or `None` when nothing can be said: a
/// driver with no such query, no render node, or a query that failed.
pub fn is_integrated(card_path: &str) -> Option<bool> {
    match driver_name(card_path)?.as_str() {
        "amdgpu" => amdgpu_is_apu(&open_render_node(card_path)?),
        // An Intel GPU with memory of its own is a discrete one.
        "i915" => Some(!i915_has_device_memory(&open_render_node(card_path)?)?),
        "xe" => Some(!xe_has_vram(&open_render_node(card_path)?)?),
        driver => {
            log::debug!("Cannot ask the {driver} driver whether its GPU is integrated");
            None
        }
    }
}

/// Returns the name of the driver bound to the given card, e.g. `amdgpu`.
fn driver_name(card_path: &str) -> Option<String> {
    let driver = fs::read_link(format!("{card_path}/device/driver")).ok()?;
    Some(driver.file_name()?.to_string_lossy().into_owned())
}

/// Opens the render node belonging to the given card, e.g. `/dev/dri/renderD128`
/// for `/sys/class/drm/card1`.
fn open_render_node(card_path: &str) -> Option<File> {
    let name = fs::read_dir(format!("{card_path}/device/drm"))
        .ok()?
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .find(|name| name.starts_with("renderD"))?;

    let path = format!("/dev/dri/{name}");
    match OpenOptions::new().read(true).write(true).open(&path) {
        Ok(node) => Some(node),
        Err(e) => {
            log::debug!("Unable to open {path}: {e}");
            None
        }
    }
}

/// Issues an ioctl against an open DRM node.
///
/// # Safety
///
/// `argp` has to point at the structure the given request expects, and stay
/// valid for the duration of the call.
unsafe fn ioctl<T>(node: &File, request: u64, argp: *mut T) -> std::io::Result<()> {
    if libc::ioctl(node.as_raw_fd(), request as _, argp) < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

// Little reads out of a driver's answer, at the offsets the uapi puts them at.
fn read_u64(answer: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_ne_bytes(answer.get(at..at + 8)?.try_into().ok()?))
}

fn read_u32(answer: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_ne_bytes(answer.get(at..at + 4)?.try_into().ok()?))
}

fn read_u16(answer: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_ne_bytes(answer.get(at..at + 2)?.try_into().ok()?))
}

// ---------------------------------------------------------------- amdgpu ---

/// `DRM_IOW(DRM_COMMAND_BASE + DRM_AMDGPU_INFO, struct drm_amdgpu_info)`,
/// which expands to `_IOW('d', 0x40 + 0x05, 32)`.
const DRM_IOCTL_AMDGPU_INFO: u64 = 0x4020_6445;
const AMDGPU_INFO_DEV_INFO: u32 = 0x16;
/// Offset of `ids_flags` within `struct drm_amdgpu_info_device`.
const AMDGPU_IDS_FLAGS_AT: usize = 136;
/// `AMDGPU_IDS_FLAGS_FUSION`, which the driver sets for every APU.
const AMDGPU_IDS_FLAGS_FUSION: u64 = 0x1;

/// Mirror of `struct drm_amdgpu_info`. The trailing array stands in for the
/// query-specific union, unused here but needed for the size the ioctl encodes.
#[repr(C)]
#[derive(Default)]
struct AmdgpuInfo {
    return_pointer: u64,
    return_size: u32,
    query: u32,
    _query_union: [u32; 4],
}
const _: () = assert!(size_of::<AmdgpuInfo>() == 32);

/// Returns whether the driver reports this GPU as an APU.
fn amdgpu_is_apu(node: &File) -> Option<bool> {
    // The driver copies back the shorter of what we ask for and what it has,
    // so a prefix of `struct drm_amdgpu_info_device` is enough.
    let mut answer = [0u8; AMDGPU_IDS_FLAGS_AT + 8];
    let mut request = AmdgpuInfo {
        return_pointer: answer.as_mut_ptr() as u64,
        return_size: answer.len() as u32,
        query: AMDGPU_INFO_DEV_INFO,
        ..Default::default()
    };

    // SAFETY: the request points at a buffer we own that outlives the call,
    // and hands the driver its length in `return_size`.
    if let Err(e) = unsafe { ioctl(node, DRM_IOCTL_AMDGPU_INFO, &mut request) } {
        log::debug!("Unable to query amdgpu device info: {e}");
        return None;
    }

    Some(read_u64(&answer, AMDGPU_IDS_FLAGS_AT)? & AMDGPU_IDS_FLAGS_FUSION != 0)
}

// ------------------------------------------------------------------ i915 ---

/// `DRM_IOWR(DRM_COMMAND_BASE + DRM_I915_QUERY, struct drm_i915_query)`,
/// which expands to `_IOWR('d', 0x40 + 0x39, 16)`.
const DRM_IOCTL_I915_QUERY: u64 = 0xc010_6479;
const DRM_I915_QUERY_MEMORY_REGIONS: u64 = 4;
/// `struct drm_i915_query_memory_regions` is a count padded out to sixteen
/// bytes, followed by 88 byte entries that each start with a memory class.
const I915_REGIONS_AT: usize = 16;
const I915_REGION_SIZE: usize = 88;
/// `I915_MEMORY_CLASS_DEVICE`, i.e. memory belonging to the GPU itself.
const I915_MEMORY_CLASS_DEVICE: u16 = 1;

/// Mirror of `struct drm_i915_query`.
#[repr(C)]
struct I915Query {
    num_items: u32,
    flags: u32,
    items_ptr: u64,
}
const _: () = assert!(size_of::<I915Query>() == 16);

/// Mirror of `struct drm_i915_query_item`. The driver writes the size of its
/// answer into `length`, or a negative errno when it does not know the query.
#[repr(C)]
#[derive(Default)]
struct I915QueryItem {
    query_id: u64,
    length: i32,
    flags: u32,
    data_ptr: u64,
}
const _: () = assert!(size_of::<I915QueryItem>() == 24);

/// Returns whether the driver reports this GPU as having memory of its own.
fn i915_has_device_memory(node: &File) -> Option<bool> {
    // Answered in two steps: the driver first says how large its answer is,
    // then fills in a buffer of that size.
    let mut item = I915QueryItem {
        query_id: DRM_I915_QUERY_MEMORY_REGIONS,
        ..Default::default()
    };
    if let Err(e) = i915_query(node, &mut item) {
        log::debug!("Unable to size the i915 memory regions query: {e}");
        return None;
    }
    if item.length <= 0 {
        log::debug!("The i915 driver does not answer the memory regions query");
        return None;
    }

    let mut answer = vec![0u8; item.length as usize];
    item.data_ptr = answer.as_mut_ptr() as u64;
    if let Err(e) = i915_query(node, &mut item) {
        log::debug!("Unable to query the i915 memory regions: {e}");
        return None;
    }

    let regions = read_u32(&answer, 0)? as usize;
    for region in 0..regions {
        let class = read_u16(&answer, I915_REGIONS_AT + region * I915_REGION_SIZE)?;
        if class == I915_MEMORY_CLASS_DEVICE {
            return Some(true);
        }
    }

    Some(false)
}

fn i915_query(node: &File, item: &mut I915QueryItem) -> std::io::Result<()> {
    let mut request = I915Query {
        num_items: 1,
        flags: 0,
        items_ptr: item as *mut I915QueryItem as u64,
    };

    // SAFETY: the request is a `struct drm_i915_query` as the ioctl expects,
    // pointing at one item owned by the caller that outlives the call.
    unsafe { ioctl(node, DRM_IOCTL_I915_QUERY, &mut request) }
}

// -------------------------------------------------------------------- xe ---

/// `DRM_IOWR(DRM_COMMAND_BASE + DRM_XE_DEVICE_QUERY, struct drm_xe_device_query)`,
/// which expands to `_IOWR('d', 0x40 + 0x00, 40)`.
const DRM_IOCTL_XE_DEVICE_QUERY: u64 = 0xc028_6440;
const DRM_XE_DEVICE_QUERY_CONFIG: u32 = 2;
/// `struct drm_xe_query_config` is a count padded out to eight bytes, followed
/// by `__u64` parameters. `DRM_XE_QUERY_CONFIG_FLAGS` is the second of them.
const XE_PARAMS_AT: usize = 8;
const XE_CONFIG_FLAGS: usize = 1;
/// `DRM_XE_QUERY_CONFIG_FLAG_HAS_VRAM`.
const XE_CONFIG_FLAG_HAS_VRAM: u64 = 1 << 0;

/// Mirror of `struct drm_xe_device_query`.
#[repr(C)]
#[derive(Default)]
struct XeDeviceQuery {
    extensions: u64,
    query: u32,
    size: u32,
    data: u64,
    reserved: [u64; 2],
}
const _: () = assert!(size_of::<XeDeviceQuery>() == 40);

/// Returns whether the driver reports this GPU as having video memory of its
/// own. This is the query switcheroo-control uses for the same purpose.
fn xe_has_vram(node: &File) -> Option<bool> {
    // Answered in two steps, as for i915.
    let mut request = XeDeviceQuery {
        query: DRM_XE_DEVICE_QUERY_CONFIG,
        ..Default::default()
    };
    if let Err(e) = xe_query(node, &mut request) {
        log::debug!("Unable to size the xe device config query: {e}");
        return None;
    }

    let mut answer = vec![0u8; request.size as usize];
    request.data = answer.as_mut_ptr() as u64;
    if let Err(e) = xe_query(node, &mut request) {
        log::debug!("Unable to query the xe device config: {e}");
        return None;
    }

    if (read_u32(&answer, 0)? as usize) <= XE_CONFIG_FLAGS {
        log::debug!("The xe device config holds no flags parameter");
        return None;
    }
    let flags = read_u64(&answer, XE_PARAMS_AT + XE_CONFIG_FLAGS * 8)?;

    Some(flags & XE_CONFIG_FLAG_HAS_VRAM != 0)
}

fn xe_query(node: &File, request: &mut XeDeviceQuery) -> std::io::Result<()> {
    // SAFETY: the request is a `struct drm_xe_device_query` as the ioctl
    // expects, owned by the caller and valid for the call.
    unsafe { ioctl(node, DRM_IOCTL_XE_DEVICE_QUERY, request) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DRM_PATH: &str = "/sys/class/drm";

    /// Every card driven by something we know how to ask, and whose render node
    /// we may open, has to give us an answer. Machines with no such card have
    /// nothing to check, which is the case on most CI runners.
    #[test]
    fn queries_every_reachable_card() {
        let Ok(cards) = fs::read_dir(DRM_PATH) else {
            return;
        };

        for card in cards.flatten() {
            let name = card.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with("card") || name.contains('-') {
                continue;
            }

            let path = format!("{DRM_PATH}/{name}");
            let Some(driver) = driver_name(&path) else {
                continue;
            };
            if !matches!(driver.as_str(), "amdgpu" | "i915" | "xe") {
                println!("{name}: driven by {driver}, skipping");
                continue;
            }
            if open_render_node(&path).is_none() {
                println!("{name}: no reachable render node, skipping");
                continue;
            }

            let is_integrated = is_integrated(&path);
            println!("{name}: driver {driver}, is_integrated = {is_integrated:?}");
            assert!(
                is_integrated.is_some(),
                "{name}: the {driver} driver did not answer the query"
            );
        }
    }
}
