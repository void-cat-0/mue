//! A local PEZ library → detail → real-track autoplay → results demo.
// This demo uploads already decoded covers; song-transition also exposes a byte loader.
#[allow(dead_code)]
#[path = "../../song-transition/src/artwork.rs"]
mod artwork;
mod assets;
mod audio;
mod chart;
#[path = "../../song-transition/src/effects.rs"]
mod effects;
mod flow;
mod gameplay;
mod menu;
#[cfg(any(not(feature = "phira-autoplay-vulkan-only"), target_os = "android"))]
#[path = "../../shared/render_backend.rs"]
mod render_backend;
mod skin;
mod ui;

use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering::Relaxed},
};

#[cfg(not(target_os = "android"))]
use std::io;

use macroquad::prelude::{Conf, next_frame};
use miniquad::{Backend, conf::RenderingBackend};
use mue_macroquad::{App, font};

pub(crate) struct Options {
    paths: Vec<PathBuf>,
    media_dirs: Vec<PathBuf>,
    silent: bool,
}

#[cfg(not(target_os = "android"))]
pub(crate) fn options() -> io::Result<Option<Options>> {
    let mut paths = Vec::new();
    let mut silent = false;
    let mut parse_flags = true;
    for arg in std::env::args_os().skip(1) {
        match (parse_flags, arg.to_str()) {
            (true, Some("--")) => parse_flags = false,
            (true, Some("--silent")) => silent = true,
            (true, Some("--help" | "-h")) => {
                println!(
                    "phira-autoplay [--silent] [PEZ ...]\nDefaults: {}\n--silent: explicit silent clock for hosts without an audio device.",
                    assets::DEFAULT_FILES.join(", ")
                );
                return Ok(None);
            }
            (true, Some(flag)) if flag.starts_with('-') => {
                return Err(io::Error::other(format!("unknown option {flag}")));
            }
            _ => paths.push(PathBuf::from(arg)),
        }
    }
    let media_dirs = assets::desktop_directories();
    if paths.is_empty() {
        paths = assets::defaults(&media_dirs);
    }
    Ok(Some(Options {
        paths,
        media_dirs,
        silent,
    }))
}

static SUSPENDED: AtomicBool = AtomicBool::new(false);

pub(crate) fn app_log(level: &str, message: &str) {
    #[cfg(target_os = "android")]
    miniquad::native::android::log_diagnostic(level, message);
    #[cfg(not(target_os = "android"))]
    if level == "ERROR" || level == "WARN" {
        eprintln!("{message}");
    } else {
        println!("{message}");
    }
}

pub(crate) async fn run(
    options: Options,
    requested: RenderingBackend,
    #[cfg(target_os = "android")] activity: &jni::objects::GlobalRef,
) -> Result<(), Box<dyn std::error::Error>> {
    let renderer = {
        let ctx = unsafe { macroquad::window::get_internal_gl() }.quad_context;
        let info = ctx.info();
        #[cfg(target_os = "android")]
        if let Err(error) = report_android_graphics_api(activity, ctx, &info.gl_version_string) {
            app_log(
                "WARN",
                &format!("Cannot update graphics API label: {error}"),
            );
        }
        info.backend
    };
    miniquad::info!("Phira Autoplay renderer: {:?}", renderer);
    let renderer_notice = match (requested, renderer) {
        (RenderingBackend::VulkanPreferred, Backend::OpenGl) => {
            Some(if cfg!(target_os = "android") {
                "Vulkan 启动失败，已回退到 GLES。设备 API 信息见右上角「导出启动日志」。"
            } else {
                "Vulkan 启动失败，已回退到 OpenGL。设备 API 信息见终端日志。"
            })
        }
        (RenderingBackend::Vulkan | RenderingBackend::VulkanPreferred, _)
            if !cfg!(target_os = "android") =>
        {
            Some("Vulkan 启动失败时，终端会输出可读取的设备 Vulkan API 信息。")
        }
        _ => None,
    };
    let mut fonts = font::fontdb::Database::new();
    fonts.load_font_data(
        include_bytes!("../../playground/assets/fonts/NotoSans-Regular.ttf").to_vec(),
    );
    fonts.load_font_data(
        include_bytes!("../../playground/assets/fonts/NotoSansCJKsc-Regular.otf").to_vec(),
    );
    fonts.set_sans_serif_family("Noto Sans CJK SC");
    font::initialize("en-US", fonts)?;
    macroquad::input::simulate_mouse_with_touch(false);
    macroquad::window::set_pause_resume_listener(|paused| {
        SUSPENDED.store(paused, Relaxed);
        if paused {
            #[cfg(target_os = "android")]
            audio::mute_for_host_pause();
            if let Err(error) = audio::pause_active() {
                miniquad::error!("Could not pause the music: {}", error);
            }
        }
    });
    let (toast_root, toasts) = mue_components::toast_host();
    let mut app = App::with_scene(menu::Library::new(
        options.paths,
        options.media_dirs,
        options.silent,
        renderer_notice,
        toasts,
    ));
    app.set_overlay_root(toast_root);

    while !app.is_exited() {
        if SUSPENDED.load(Relaxed) {
            app.try_frame_paused()?;
        } else {
            app.try_frame()?;
        }
        if !app.is_exited() {
            next_frame().await;
        }
    }
    Ok(())
}

pub(crate) fn window_config() -> Conf {
    #[cfg(all(feature = "phira-autoplay-vulkan-only", not(target_os = "android")))]
    let platform = miniquad::conf::Platform {
        rendering_backend: RenderingBackend::Vulkan,
        ..Default::default()
    };
    #[cfg(any(not(feature = "phira-autoplay-vulkan-only"), target_os = "android"))]
    let platform = render_backend::platform();

    Conf {
        window_title: "Mue · Phira Autoplay".into(),
        window_width: 1140,
        window_height: 700,
        fullscreen: cfg!(target_os = "android"),
        high_dpi: true,
        platform,
        ..Default::default()
    }
}

#[cfg(target_os = "android")]
static FORCE_GLES: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "android")]
static ANDROID_HOST: std::sync::Mutex<Option<(PathBuf, jni::objects::GlobalRef)>> =
    std::sync::Mutex::new(None);

#[cfg(target_os = "android")]
fn set_android_host(directory: PathBuf, activity: jni::objects::GlobalRef) {
    *ANDROID_HOST.lock().unwrap() = Some((directory, activity));
}

#[cfg(target_os = "android")]
fn set_host_paused(paused: bool) {
    if paused {
        audio::mute_for_host_pause();
    }
    SUSPENDED.store(paused, Relaxed);
}

#[cfg(target_os = "android")]
fn android_main() {
    let Some((directory, activity)) = ANDROID_HOST.lock().unwrap().take() else {
        miniquad::error!("The Android host did not register a chart directory");
        return;
    };
    let options = Options {
        paths: assets::DEFAULT_FILES
            .map(|file| directory.join(file))
            .into(),
        media_dirs: vec![directory],
        silent: false,
    };
    let mut config = window_config();
    if FORCE_GLES.load(Relaxed) {
        config.platform.rendering_backend = RenderingBackend::OpenGl;
        app_log(
            "WARN",
            "GLES mode was selected from the diagnostic controls.",
        );
    }
    let requested = config.platform.rendering_backend;
    macroquad::Window::from_config(config, async move {
        if let Err(error) = run(options, requested, &activity).await {
            miniquad::error!("Phira Autoplay: {}", error);
        }
        if let Err(error) = request_finish(&activity) {
            miniquad::error!("Cannot finish the Phira Autoplay activity: {}", error);
        }
    });
}

#[cfg(target_os = "android")]
fn report_android_graphics_api(
    activity: &jni::objects::GlobalRef,
    ctx: &mut dyn miniquad::RenderingBackend,
    gl_version: &str,
) -> jni::errors::Result<()> {
    use ash::vk::{api_version_major, api_version_minor, api_version_patch};
    use miniquad::graphics::vulkan::VulkanRenderingMode;

    let label = if let Some(vulkan) = ctx.vulkan_context() {
        let api = vulkan.api_version();
        let device_api = vulkan.physical_device_api_version();
        let mode = match vulkan.rendering_mode() {
            VulkanRenderingMode::Legacy => "传统 render pass",
            VulkanRenderingMode::DynamicKhr => "KHR 动态渲染",
            VulkanRenderingMode::DynamicCore => "核心动态渲染",
        };
        format!(
            "Vulkan {}.{} · {mode}\nGPU 报告 Vulkan {}.{}.{}",
            api_version_major(api),
            api_version_minor(api),
            api_version_major(device_api),
            api_version_minor(device_api),
            api_version_patch(device_api),
        )
    } else {
        app_log("INFO", &format!("GL_VERSION: {gl_version}"));
        match gl_version
            .strip_prefix("OpenGL ES ")
            .and_then(|s| s.split_whitespace().next())
        {
            Some(version) => format!("OpenGL ES {version}"),
            None => "OpenGL ES（版本未知）".into(),
        }
    };
    app_log(
        "INFO",
        &format!("Active graphics API: {}", label.replace('\n', "; ")),
    );

    let vm = unsafe { jni::JavaVM::from_raw(ndk_context::android_context().vm().cast())? };
    let mut env = vm.attach_current_thread()?;
    let text = env.new_string(label)?;
    env.call_method(
        activity.as_obj(),
        "setGraphicsApi",
        "(Ljava/lang/String;)V",
        &[jni::objects::JValue::Object(&text)],
    )?;
    Ok(())
}

#[cfg(target_os = "android")]
fn request_finish(activity: &jni::objects::GlobalRef) -> jni::errors::Result<()> {
    let context = ndk_context::android_context();
    // The Java Activity initialized the process-wide VM handle before quad_main.
    let vm = unsafe { jni::JavaVM::from_raw(context.vm().cast())? };
    let mut env = vm.attach_current_thread()?;
    env.call_method(activity.as_obj(), "requestFinish", "()V", &[])?;
    Ok(())
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn quad_main() {
    android_main();
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_quad_1native_QuadNative_setForceGles(
    _env: jni::JNIEnv<'_>,
    _class: jni::objects::JClass<'_>,
    force_gles: jni::sys::jboolean,
) {
    FORCE_GLES.store(force_gles != 0, Relaxed);
    app_log(
        "INFO",
        if force_gles != 0 {
            "Next renderer selection: GLES."
        } else {
            "Next renderer selection: automatic Vulkan preference."
        },
    );
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_quad_1native_QuadNative_setChartDirectory<'local>(
    mut env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    activity: jni::objects::JObject<'local>,
    directory: jni::objects::JString<'local>,
) {
    let register = (|| {
        let path = String::from(env.get_string(&directory)?);
        set_android_host(path.into(), env.new_global_ref(activity)?);
        Ok::<_, jni::errors::Error>(())
    })();
    if let Err(error) = register {
        miniquad::error!("Cannot register the chart directory: {}", error);
    }
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_quad_1native_QuadNative_setHostPaused(
    _env: jni::JNIEnv<'_>,
    _class: jni::objects::JClass<'_>,
    paused: jni::sys::jboolean,
) {
    set_host_paused(paused != 0);
}
