use miniquad::conf::RenderingBackend;

/// Choose before creating the native window. A positive probe is not a check
/// of the actual surface, logical device, or swapchain initialization.
pub fn preferred() -> RenderingBackend {
    #[cfg(all(
        feature = "vulkan",
        any(
            windows,
            target_os = "android",
            all(target_os = "linux", not(target_env = "ohos"))
        )
    ))]
    if vulkan::has_candidate() {
        return RenderingBackend::Vulkan;
    }
    RenderingBackend::OpenGl
}

#[cfg(all(
    feature = "vulkan",
    any(
        windows,
        target_os = "android",
        all(target_os = "linux", not(target_env = "ohos"))
    )
))]
mod vulkan {
    use ash::{
        Entry, Instance,
        extensions::khr::{DynamicRendering, Surface, Swapchain},
        vk,
    };
    use std::ffi::CStr;

    pub(super) fn has_candidate() -> bool {
        let Ok(entry) = (unsafe { Entry::load() }) else {
            return false;
        };
        let Ok(Some(loader_version)) = entry.try_enumerate_instance_version() else {
            return false;
        };
        if loader_version < vk::API_VERSION_1_2 {
            return false;
        }
        let api_version = loader_version.min(vk::API_VERSION_1_3);
        let application = vk::ApplicationInfo::builder().api_version(api_version);
        let extensions = [
            Surface::name().as_ptr(),
            #[cfg(target_os = "android")]
            ash::extensions::khr::AndroidSurface::name().as_ptr(),
        ];
        let create_info = vk::InstanceCreateInfo::builder()
            .application_info(&application)
            .enabled_extension_names(&extensions);
        unsafe {
            let Ok(instance) = entry.create_instance(&create_info, None) else {
                return false;
            };
            let found = instance.enumerate_physical_devices().is_ok_and(|devices| {
                devices
                    .into_iter()
                    .any(|device| is_candidate(&instance, device, api_version))
            });
            instance.destroy_instance(None);
            found
        }
    }

    fn is_candidate(instance: &Instance, device: vk::PhysicalDevice, api_version: u32) -> bool {
        let version = unsafe { instance.get_physical_device_properties(device) }
            .api_version
            .min(api_version);
        if version < vk::API_VERSION_1_2 {
            return false;
        }
        let Ok(extensions) = (unsafe { instance.enumerate_device_extension_properties(device) })
        else {
            return false;
        };
        let has_extension = |name: &CStr| {
            extensions.iter().any(|extension| {
                (unsafe { CStr::from_ptr(extension.extension_name.as_ptr()) }) == name
            })
        };
        if !has_extension(Swapchain::name())
            || (version < vk::API_VERSION_1_3 && !has_extension(DynamicRendering::name()))
        {
            return false;
        }

        let mut dynamic_rendering = vk::PhysicalDeviceDynamicRenderingFeatures::default();
        let mut features = vk::PhysicalDeviceFeatures2::builder()
            .push_next(&mut dynamic_rendering)
            .build();
        unsafe { instance.get_physical_device_features2(device, &mut features) };
        dynamic_rendering.dynamic_rendering == vk::TRUE
            && unsafe { instance.get_physical_device_queue_family_properties(device) }
                .iter()
                .any(|queue| queue.queue_flags.contains(vk::QueueFlags::GRAPHICS))
    }
}
