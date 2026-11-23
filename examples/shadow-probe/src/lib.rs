//! Native Android entry for the Vulkan shadow probe.

#[cfg(target_os = "android")]
#[path = "main.rs"]
mod probe;

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn quad_main() {
    probe::main();
}
