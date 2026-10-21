# HM1-Rust

A Rust re-implementation of **Hitman: Codename 47**'s launcher (`Hitman.Exe`)
plus a **Vulkan** implementation of the DirectDraw 7 / Direct3D 7 API the game
renders with, so the game runs on Vulkan at up to **3840×2160, 32-bit**.

This project has no files from the Hitman: Codename 47 game. You must have the full paid version of Hitman: Codename 47 to run the HitMan-Rust.exe after building.

Hitman: Codename 47 is owned by IO Interactive A/S.

Some possible updates coming:

- Scale menu and UI by percent

- POV change

- Better WASD controls, shift run any direction

```
HitMan-Rust.exe            (launcher/)  Rust port of Hitman.Exe's WinMain
HitManVK\ddraw.dll         (hm1vk/)     DirectDraw 7 + Direct3D 7 on Vulkan
HitManVK\hm1vk.ini                       renderer options
HitMan-Rust.ini                          game config (Resolution 3840x2160, ColorDepth 32)
```

## How it fits together

The original game is a small launcher plus plug-in DLLs (see `re/NOTES.md`):

```
Hitman.Exe ──LoadLibrary──> System.dll (engine) ──DrawDll──> RenderD3D.dll ──COM──> DirectDraw7/Direct3D7
                 │                     └── SoundDll, ScriptDll, LocaleDll, EngineData.dll, HitmanDlc.dlc, …
                 └── Globals.dll (g_pSysInterface)
```

* **HitMan-Rust.exe** does what the unpacked `Hitman.Exe` does (single-instance
  mutex, crash signal handlers, `LoadLibrary("System.dll")`, `@<ini>`
  argument, `g_pSysInterface->vtbl[0xC8]`), with the same PE characteristics
  the 2000-era engine expects (fixed base 0x400000, no ASLR/DEP, not
  large-address-aware). It also enables per-monitor DPI awareness so 4K is
  real pixels. Before starting the engine it pre-loads `HitManVK\ddraw.dll`,
  so RenderD3D.dll's `DDRAW.dll` import binds to the Vulkan implementation
  instead of the system one.
* **hm1vk (ddraw.dll)** implements every DirectDraw 7 / Direct3D 7 interface the
  engine uses: `IDirectDraw7`, `IDirectDrawSurface7`, `IDirectDrawPalette`,
  `IDirectDrawClipper`, `IDirectDrawGammaControl`, `IDirect3D7`,
  `IDirect3DDevice7`, `IDirect3DVertexBuffer7`. Draw calls are translated to
  Vulkan 1.3 (dynamic rendering, extended dynamic state, push descriptors) and
  rendered by a fixed-function emulation shader (`hm1vk/shaders/ff.wgsl`):
  pre-transformed and transformed vertices, D3D7 lighting with 8 lights, texgen
  and texture transforms, 8 texture-blend stages, table/vertex fog, alpha
  test, colour key, stencil, depth bias. DXT1-5 textures are uploaded as BC1-3.
  The swapchain is presented with the DirectDraw gamma ramp applied.
* **4K / widescreen.** The original RenderD3D.dll forces fullscreen
  resolutions onto a 4:3 table capped at 1600×1200. Any wider mode fails with
  *"Unable to find a suitable display mode"*. hm1vk restores the .ini
  `Resolution` in the engine's settings before mode selection
  (`hm1vk/src/engine.rs`, `FixResolution=1`), so 1280×720, 1920×1080,
  2560×1440 and 3840×2160 work.

`HitMan-Rust.exe -d3d` skips the Vulkan layer and uses the system Direct3D
(original behaviour, including the 4:3 limit).

## Build

Requirements: Rust (stable, MSVC toolchain), Visual Studio Build Tools (link.exe), a
Vulkan 1.3 driver. No Vulkan SDK is needed: shaders are compiled from WGSL to
SPIR-V by `naga` in `hm1vk/build.rs`.

```bat
tools\build.bat            :: rustup target add i686-pc-windows-msvc + cargo build --release -> dist\
tools\deploy.bat [gamedir] :: copy dist\ into the game folder
```

The game is 32-bit, so everything builds for `i686-pc-windows-msvc` with a
static CRT (`RUSTFLAGS=-C target-feature=+crt-static`). Building for any
other target stops with a compile error. Without `build.bat`:

```bat
set RUSTFLAGS=-C target-feature=+crt-static
cargo build --release --target i686-pc-windows-msvc
```

## Install (files to copy into the game folder)

Copy these four files into `…\steamapps\common\Hitman Codename 47\`:

| File | Purpose |
|---|---|
| `HitMan-Rust.exe` | launcher; start the game with this |
| `HitManVK\ddraw.dll` | Vulkan DirectDraw/Direct3D 7 renderer (keep it in the `HitManVK` sub-folder; do **not** put it next to the exe) |
| `HitManVK\hm1vk.ini` | renderer options (optional; defaults apply without it) |
| `HitMan-Rust.ini` | game config used by HitMan-Rust.exe (optional; without it the stock `Hitman.ini` is used) |

No original file is replaced. The game's own DLLs and data (System.dll,
Globals.dll, RenderD3D.dll, EngineData.dll, HitmanDlc.dlc, gsc.dll, Sound.dll,
EAX.dll, a3d.dll, Locale.dll, xmlparse.dll, the level folders, *.zip/ANM/DXT/PAL/WAV
packs) are used as they are.

## Options

`HitMan-Rust.ini` uses the engine's own format (`Resolution 3840x2160`,
`ColorDepth 32`, `Window 1` for windowed, `DefaultScene C1_HongKong\C1_1.zip`
to jump to a mission, …).

`HitManVK\hm1vk.ini`:

| Key | Default | |
|---|---|---|
| VSync | 1 | wait for vblank |
| FpsLimit | 0 | frame cap (0 = none) |
| ChangeDisplayMode | 1 | switch the desktop resolution for fullscreen modes like real DirectDraw; 0 = keep desktop and scale |
| FixResolution | 1 | honour the .ini resolution (16:9, >1600×1200) |
| Anisotropy | 0 | force anisotropic filtering (2-16) |
| MaxWidth / MaxHeight | 3840 / 2160 | largest mode offered |
| ExtraModes | 1280x720, 1920x1080, 2560x1440, 3840x2160 | always-offered modes |
| Modes16Bit | 1 | also offer 16-bit modes (rendered in 32-bit) |
| Validation | 0 | Vulkan validation layer |
| MenuZoom | 1 | scale the engine's fixed 640×480 menu/laptop screens to fill the display |
| MenuZoomClipCursor | 1 | keep the mouse inside the zoomed menu panel |
| LogLevel | 0 | log detail for `HitManVK\hm1vk.log` (0 info, 1 debug, 2 trace) |

## Testing

`tools\test-run.ps1` deploys, starts the game, captures screenshots and
collects `hm1vk.log`. `tools\test-scenes.ps1` boots individual missions via
`DefaultScene` (training, Hong Kong, Rotterdam, Budapest, Colombia,
sanitarium, laptop briefing, menus).

## Reverse engineering

`re/` contains the Ghidra headless driver (`ghidra_analyze.bat`,
`ghidra_scripts/ExportAll.java`), the unpacker (`dump_unpacked.ps1`) and
`NOTES.md` with the findings this implementation is based on.

## Known limitations

* The engine's front-end screens (menus, options, laptop briefing) are a fixed
  640×480 panel. `MenuZoom` scales them up to the full display height.
* The in-game HUD and subtitles are positioned in absolute pixels, so they are
  small at 3840×2160. This is how the original engine behaves at high
  resolutions.
* Pre-mission scenes (`*_Pre.zip`) cannot be booted directly with
  `DefaultScene`; the original renderer behaves the same.
* Environment bump mapping (EMBM) is not advertised, so the engine uses its
  non-bump path.
* Point sprites, user clip planes and vertex blending are not used by the game
  and are not implemented.
