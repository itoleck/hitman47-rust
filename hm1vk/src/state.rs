//! Process-wide state shared by all DirectDraw/Direct3D objects.
//!
//! DirectDraw 7 objects of one process share a display, a window and (here)
//! one Vulkan device. Hitman drives all rendering from one thread, so the
//! state is kept in a single global without locking; API entry points obtain
//! it with [`world()`].

use std::path::PathBuf;
use std::time::Instant;

use crate::config::Config;
use crate::surface::Surface;
use crate::types::DDGAMMARAMP;
use crate::vk::Gpu;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mode {
    pub width: u32,
    pub height: u32,
    pub bpp: u32,
    pub refresh: u32,
}

pub struct World {
    pub config: Config,
    pub dir: PathBuf,
    gpu: Option<Gpu>,
    gpu_failed: bool,
    pub hwnd: isize,
    pub coop: u32,
    /// Mode set with SetDisplayMode (None = desktop / windowed).
    pub mode: Option<Mode>,
    pub desktop: Mode,
    pub display_changed: bool,
    pub primary: *mut Surface,
    pub gamma: DDGAMMARAMP,
    pub gamma_active: bool,
    pub gamma_dirty: bool,
    pub last_present: Option<Instant>,
    pub frame_count: u64,
    pub fps_timer: Option<Instant>,
}

static mut WORLD: Option<World> = None;
static mut CONFIG_FALLBACK: Option<Config> = None;

pub fn init(dir: PathBuf, config: Config) {
    let desktop = crate::display::desktop_mode();
    unsafe {
        let w = &raw mut WORLD;
        *w = Some(World {
            config,
            dir,
            gpu: None,
            gpu_failed: false,
            hwnd: 0,
            coop: 0,
            mode: None,
            desktop,
            display_changed: false,
            primary: std::ptr::null_mut(),
            gamma: DDGAMMARAMP::identity(),
            gamma_active: false,
            gamma_dirty: false,
            last_present: None,
            frame_count: 0,
            fps_timer: None,
        });
    }
}

/// The global state. Single-threaded use only (see module docs).
#[allow(static_mut_refs)]
pub fn world() -> &'static mut World {
    unsafe {
        if WORLD.is_none() {
            init(PathBuf::from("."), Config::default());
        }
        WORLD.as_mut().unwrap()
    }
}

#[allow(static_mut_refs)]
pub fn config() -> &'static Config {
    unsafe {
        if let Some(w) = WORLD.as_ref() {
            return &w.config;
        }
        CONFIG_FALLBACK.get_or_insert_with(Config::default)
    }
}

impl World {
    /// The Vulkan device, created on first use.
    pub fn gpu(&mut self) -> Option<&mut Gpu> {
        if self.gpu.is_none() && !self.gpu_failed {
            match Gpu::new(self.config.validation) {
                Ok(g) => self.gpu = Some(g),
                Err(e) => {
                    crate::info!("Vulkan initialisation failed: {e}");
                    self.gpu_failed = true;
                    crate::display::error_box(&format!(
                        "HitMan-Rust could not initialise Vulkan:\n{e}\n\nStart the game with -d3d to use Direct3D instead."
                    ));
                }
            }
        }
        self.gpu.as_mut()
    }

    pub fn gpu_if_ready(&mut self) -> Option<&mut Gpu> {
        self.gpu.as_mut()
    }

    /// Current render size: the display mode, or the desktop.
    pub fn screen_mode(&self) -> Mode {
        self.mode.unwrap_or(self.desktop)
    }

    pub fn is_fullscreen(&self) -> bool {
        self.coop & crate::types::DDSCL_FULLSCREEN != 0
    }

    pub fn shutdown(&mut self) {
        if let Some(mut g) = self.gpu.take() {
            g.wait_idle();
            drop(g);
        }
        if self.display_changed {
            crate::display::restore_display_mode();
            self.display_changed = false;
        }
    }
}
