#[cfg(any(windows, all(target_os = "linux", not(target_env = "ohos"))))]
mod generator {
    use miniquad::{
        ShaderMeta, UniformBlockLayout, UniformDesc, UniformType, vulkan::compile_glsl,
    };
    use std::{
        fs, io,
        path::{Path, PathBuf},
    };

    type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

    struct Shader {
        name: &'static str,
        vertex: PathBuf,
        fragment: PathBuf,
        meta: ShaderMeta,
    }

    fn shader(
        name: &'static str,
        dir: &Path,
        vertex: &str,
        fragment: &str,
        uniforms: &[(&str, UniformType)],
        images: &[&str],
    ) -> Shader {
        Shader {
            name,
            vertex: dir.join(vertex),
            fragment: dir.join(fragment),
            meta: ShaderMeta {
                uniforms: UniformBlockLayout {
                    uniforms: uniforms
                        .iter()
                        .map(|&(name, ty)| UniformDesc::new(name, ty))
                        .collect(),
                },
                images: images.iter().map(|&name| name.to_owned()).collect(),
            },
        }
    }

    fn shaders() -> Vec<Shader> {
        use UniformType::{Float1, Float4, Mat4};

        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let macroquad = workspace.join("../macroquad/src/shaders/vulkan");
        let transitions = workspace.join("mue-macroquad/src/scene_transition/shaders/vulkan");
        let quad_uniforms = [("Projection", Mat4), ("Model", Mat4), ("_Time", Float4)];
        let quad_images = ["Texture", "_ScreenTexture"];
        vec![
            shader(
                "default",
                &macroquad,
                "default.vert.glsl",
                "default.frag.glsl",
                &quad_uniforms,
                &quad_images,
            ),
            shader(
                "snapshotter",
                &macroquad,
                "snapshotter.vert.glsl",
                "snapshotter.frag.glsl",
                &[],
                &["Texture"],
            ),
            shader(
                "ui",
                &macroquad,
                "ui.vert.glsl",
                "ui.frag.glsl",
                &quad_uniforms,
                &quad_images,
            ),
            shader(
                "retained",
                &macroquad,
                "retained.vert.glsl",
                "retained.frag.glsl",
                &[("Model", Mat4), ("Projection", Mat4), ("Alpha", Float1)],
                &["Texture"],
            ),
            // QuadGl prepends its standard uniforms and reserves _ScreenTexture
            // even when the shader does not sample it.
            shader(
                "picture-copy",
                &transitions,
                "picture.vert.glsl",
                "picture-copy.frag.glsl",
                &quad_uniforms,
                &quad_images,
            ),
            shader(
                "picture-opaque",
                &transitions,
                "picture.vert.glsl",
                "picture-opaque.frag.glsl",
                &quad_uniforms,
                &quad_images,
            ),
            shader(
                "picture-crossfade",
                &transitions,
                "picture.vert.glsl",
                "picture-crossfade.frag.glsl",
                &[
                    ("Projection", Mat4),
                    ("Model", Mat4),
                    ("_Time", Float4),
                    ("Progress", Float1),
                ],
                &["Texture", "_ScreenTexture", "From"],
            ),
        ]
    }

    fn compile(shader: Shader) -> Result<[(PathBuf, Vec<u8>); 2]> {
        let vertex = fs::read_to_string(&shader.vertex)?;
        let fragment = fs::read_to_string(&shader.fragment)?;
        let (vertex, fragment) = compile_glsl(&vertex, &fragment, &shader.meta)
            .map_err(|error| io::Error::other(format!("{}: {error}", shader.name)))?;
        let output_dir = shader.vertex.parent().unwrap().join("spirv");
        Ok(
            [("vert", vertex), ("frag", fragment)].map(|(stage, words)| {
                let path = output_dir.join(format!("{}.{stage}.spv", shader.name));
                let bytes = words.into_iter().flat_map(u32::to_le_bytes).collect();
                (path, bytes)
            }),
        )
    }

    /// Regenerate in the normal mode; `--check` recompiles everything and checks
    /// generated files byte-for-byte against the committed assets.
    pub fn run() -> Result<()> {
        // Run without arguments to regenerate assets; --check enforces byte-for-byte reproducibility.
        let check = match std::env::args().skip(1).collect::<Vec<_>>().as_slice() {
            [] => false,
            [flag] if flag == "--check" => true,
            _ => return Err("usage: precompile-vulkan-shaders [--check]".into()),
        };
        let shaders = shaders()
            .into_iter()
            .map(compile)
            .collect::<Result<Vec<_>>>()?;
        for (path, bytes) in shaders.into_iter().flatten() {
            if check {
                let actual = fs::read(&path)
                    .map_err(|error| io::Error::other(format!("{}: {error}", path.display())))?;
                if actual != bytes {
                    return Err(
                        format!("{} is stale, run the generator again", path.display()).into(),
                    );
                }
            } else {
                fs::create_dir_all(path.parent().unwrap())?;
                fs::write(&path, bytes)?;
                println!("{}", path.display());
            }
        }
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(any(windows, all(target_os = "linux", not(target_env = "ohos"))))]
    return generator::run();
    #[cfg(not(any(windows, all(target_os = "linux", not(target_env = "ohos")))))]
    Err("run this generator on a Windows or Linux desktop target".into())
}
