//! Android native-library entry for the strict Vulkan shader smoke example.

#[cfg(target_os = "android")]
#[path = "main.rs"]
mod smoke;

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn quad_main() {
    smoke::main();
}
