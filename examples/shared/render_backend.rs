use miniquad::conf::{Platform, RenderingBackend};

const VULKAN_COMPILED: bool = cfg!(all(
    feature = "vulkan",
    any(
        windows,
        target_os = "android",
        all(target_os = "linux", not(target_env = "ohos"))
    )
));

fn select_backend(mode: Option<&str>) -> Result<RenderingBackend, &'static str> {
    match mode.unwrap_or("auto") {
        "auto" if VULKAN_COMPILED => Ok(RenderingBackend::VulkanPreferred),
        "auto" | "opengl" | "gles" => Ok(RenderingBackend::OpenGl),
        "vulkan" if VULKAN_COMPILED => Ok(RenderingBackend::Vulkan),
        "vulkan" => Err(
            "MUE_RENDER_BACKEND=vulkan requires the vulkan feature on Windows, desktop Linux or Android",
        ),
        _ => Err("MUE_RENDER_BACKEND must be auto, opengl (or gles), or vulkan"),
    }
}

/// Select the renderer when starting these examples. In auto mode Miniquad
/// tries Vulkan on the real window and starts GL/GLES if initialization fails.
pub fn platform() -> Platform {
    let mode = std::env::var("MUE_RENDER_BACKEND").ok();
    let mode = mode.as_deref();
    // APK launchers usually don't inherit an environment from the build shell.
    #[cfg(target_os = "android")]
    let mode = mode.or(option_env!("MUE_RENDER_BACKEND"));

    Platform {
        rendering_backend: select_backend(mode).unwrap_or_else(|error| panic!("{error}")),
        #[cfg(all(target_os = "linux", not(target_env = "ohos")))]
        linux_backend: {
            use miniquad::conf::LinuxBackend;
            if std::env::var_os("DISPLAY").is_none()
                && (std::env::var_os("WAYLAND_DISPLAY").is_some()
                    || std::env::var_os("WAYLAND_SOCKET").is_some())
            {
                LinuxBackend::WaylandWithX11Fallback
            } else {
                LinuxBackend::X11WithWaylandFallback
            }
        },
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_prefers_vulkan_only_when_compiled() {
        let expected = if VULKAN_COMPILED {
            RenderingBackend::VulkanPreferred
        } else {
            RenderingBackend::OpenGl
        };
        assert_eq!(select_backend(None), Ok(expected));
        assert_eq!(select_backend(Some("auto")), Ok(expected));
    }

    #[test]
    fn explicit_backend_does_not_fall_back() {
        for mode in ["opengl", "gles"] {
            assert_eq!(select_backend(Some(mode)), Ok(RenderingBackend::OpenGl));
        }
        if VULKAN_COMPILED {
            assert_eq!(select_backend(Some("vulkan")), Ok(RenderingBackend::Vulkan));
        } else {
            assert!(select_backend(Some("vulkan")).is_err());
        }
        assert!(select_backend(Some("vulcan")).is_err());
    }
}
