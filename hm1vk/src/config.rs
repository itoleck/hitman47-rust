//! Runtime options, read from `HitManVK\hm1vk.ini` (key = value lines).

use std::path::Path;

#[derive(Clone, Debug)]
pub struct Config {
    /// Wait for vertical blank when presenting.
    pub vsync: bool,
    /// Frame-rate cap (0 = none). The engine was tuned for <= 60 fps.
    pub fps_limit: u32,
    /// `true`: change the desktop resolution for fullscreen modes, like real
    /// DirectDraw exclusive mode. `false`: keep the desktop resolution and scale.
    pub change_display_mode: bool,
    /// Force anisotropic filtering level on all linear-filtered textures (0 = off).
    pub force_anisotropy: u32,
    /// Enable the Vulkan validation layer.
    pub validation: bool,
    /// Log verbosity: 0 = info, 1 = debug, 2 = trace.
    pub log_level: u32,
    /// Maximum display mode width/height advertised (defaults to 4K UHD).
    pub max_width: u32,
    pub max_height: u32,
    /// Advertise these extra modes even if the monitor does not list them
    /// (they are rendered at that size and scaled to the display).
    pub extra_modes: Vec<(u32, u32)>,
    /// Advertise 16-bit display modes as well as 32-bit ones.
    pub modes_16bit: bool,
    /// Restore the .ini resolution that RenderD3D's 4:3 table overrides
    /// (needed for 16:9 and anything above 1600x1200, e.g. 3840x2160).
    pub fix_resolution: bool,
    /// Scale factor for the game's 2D interface (menus and HUD), 1.0..=3.0
    /// (`UIZoom = 1x/2x/3x` or `100%/200%/300%`, see ui.rs).
    pub ui_zoom: f32,
    /// Run key modifies every movement direction (input.rs).
    pub modern_run: bool,
    /// Horizontal field of view of the gameplay camera in degrees
    /// (default 67.4 = the game's own; 57.4..87.4, see fov.rs).
    pub fov: f32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            vsync: true,
            fps_limit: 0,
            change_display_mode: true,
            force_anisotropy: 0,
            validation: false,
            log_level: 0,
            max_width: 3840,
            max_height: 2160,
            extra_modes: vec![(1280, 720), (1920, 1080), (2560, 1440), (3840, 2160)],
            modes_16bit: true,
            fix_resolution: true,
            ui_zoom: 1.0,
            modern_run: true,
            fov: crate::fov::DEFAULT_FOV,
        }
    }
}

fn parse_bool(v: &str) -> bool {
    matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

impl Config {
    pub fn load(dir: &Path) -> Config {
        let mut c = Config::default();
        let Ok(text) = std::fs::read_to_string(dir.join("hm1vk.ini")) else {
            return c;
        };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') || line.starts_with('[') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
            match k.as_str() {
                "vsync" => c.vsync = parse_bool(v),
                "fpslimit" => c.fps_limit = v.parse().unwrap_or(0),
                "changedisplaymode" => c.change_display_mode = parse_bool(v),
                "anisotropy" => c.force_anisotropy = v.parse().unwrap_or(0),
                "validation" => c.validation = parse_bool(v),
                "loglevel" => c.log_level = v.parse().unwrap_or(0),
                "maxwidth" => c.max_width = v.parse().unwrap_or(3840),
                "maxheight" => c.max_height = v.parse().unwrap_or(2160),
                "modes16bit" => c.modes_16bit = parse_bool(v),
                "fixresolution" => c.fix_resolution = parse_bool(v),
                "modernrun" => c.modern_run = parse_bool(v),
                "uizoom" => c.ui_zoom = crate::ui::parse_scale(v).unwrap_or(1.0),
                "fov" => c.fov = crate::fov::clamp(v.parse().unwrap_or(crate::fov::DEFAULT_FOV)),
                "extramodes" => {
                    c.extra_modes = v
                        .split(',')
                        .filter_map(|m| {
                            let (w, h) = m.trim().split_once('x')?;
                            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
                        })
                        .collect()
                }
                _ => {}
            }
        }
        c
    }
}
