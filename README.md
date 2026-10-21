# HitMan-Rust

A Rust re-implementation of **Hitman: Codename 47**'s launcher (`Hitman.Exe`)
plus a **Vulkan** implementation of the DirectDraw 7 / Direct3D 7 API the game
renders with, so the game runs on Vulkan at up to **3840×2160, 32-bit**.

This project has no files from the Hitman: Codename 47 game. You must have the full paid version of Hitman: Codename 47 to run the HitMan-Rust.exe after building.

Hitman: Codename 47 is owned by IO Interactive A/S.

What's in the release:
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
tools\build.bat   :: rustup target add i686-pc-windows-msvc + cargo build --release -> dist\
```

Each run of `build.bat` adds 1 to the patch number in `version.txt`
(`0.0.1` → `0.0.2` → …) and stamps it as the file and product version of both
binaries (`build-support/version_resource.rs`, used by both build scripts):

| | `HitMan-Rust.exe` | `HitManVK\ddraw.dll` |
|---|---|---|
| File description | Hitman Codename 47 Rust Implementation | Hitman Codename 47 Rust library |
| Product name | Hitman-Rust | Hitman-Rust |
| Copyright | MIT 2026 | MIT 2026 |
| Original filename | HitMan-Rust.exe | ddraw.dll |

The version is also written at the top of `HitManVK\hm1vk.log`. A failed build puts the
old number back. Edit `version.txt` to change the major or minor number. The
resource is compiled with the Windows SDK's `rc.exe`, which is found
automatically (set `RC` to its full path to override).

```powershell
.\tools\deploy.ps1                                  # copy dist\ into the game folder
.\tools\deploy.ps1 -Game 'D:\Games\Hitman Codename 47' # different game folder
```

`deploy.ps1` defaults `-Game` to
`E:\Games\SteamRoot\Steam\steamapps\common\Hitman Codename 47`. It always copies
`HitMan-Rust.exe` and `HitManVK\ddraw.dll`, and copies the two .ini files only
if they are not already there, so your settings are kept. From `cmd.exe`, or if
script execution is disabled, run it as
`powershell -ExecutionPolicy Bypass -File tools\deploy.ps1 [-Game <folder>]`.

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
| UIZoom | 1x | scale of the menus, laptop briefing, HUD and subtitles: `1x`/`2x`/`3x` or `100%`/`200%`/`300%` (see below) |
| ModernRun | 1 | run key modifies every direction (Shift+W/S, with A/D for diagonals; see below); 0 = original controls |
| FOV | 67.4 | horizontal field of view of the gameplay camera in degrees, 57.4–87.4 (see below) |
| LogLevel | 0 | log detail for `HitManVK\hm1vk.log` (0 info, 1 debug, 2 trace) |

### UI zoom

The engine draws its whole 2D interface in absolute pixels: the menus are a
640×480 panel and the HUD is a few small boxes in the corners, which is tiny
at 3840×2160. `UIZoom` scales it (`hm1vk/src/ui.rs`). hm1vk recognises the
engine's UI camera (a 45° perspective camera with an identity view) and scales
those draws by enlarging their viewport about an anchor point:

* **Menus** (options screens, laptop briefing: frames that contain only UI)
  are zoomed about the screen centre, limited so
  the panel still fits the screen (e.g. at most 2.25x for 640×480 at 1080p).
  The game moves its own cursor by the mouse offset from the screen centre and
  draws it with the menu, so it is zoomed too and clicks land on what it
  points at. hm1vk scales the mouse movement by 1/zoom so the cursor follows
  the mouse 1:1, and stops it at the edge of the visible part of the screen.
* **In game**, each HUD element is scaled towards the screen edge or corner it
  sits at (health top-left, weapon top-right, subtitles bottom-left, centred
  messages about the centre). The 3D scene is not affected.
* The game also uses the UI camera for some 3D scenes (Hitman's body on the
  death screen). Only flat draws count as interface elements, and while a
  large 3D model is drawn with the UI camera nothing is scaled, so those
  scenes look as in the original.

### Modern run controls

The original controls run only forwards: the run key (Shift) on its own makes
Hitman run forwards, and with S he still walks backwards. The engine
can run backwards (`Run_Backward`, normally bound to the unused
`RunBackward` action) and run diagonally (`Run_Str_Left/Right`); it picks those
from the actions it reads every frame. With `ModernRun = 1` hm1vk hooks the
engine's action query (`hm1vk/src/input.rs`) and answers the movement actions
from the keys that are held:

| Keys | Movement |
|---|---|
| W | walk forwards |
| Shift + W | run forwards |
| Shift + W + A/D | run diagonally forwards |
| S | walk backwards |
| Shift + S | run backwards |
| Shift + S + A/D | run diagonally backwards |
| A/D, Shift + A/D | strafe (the game has no sideways run) |
| Shift alone | nothing |

The actions are remapped, not keys, so rebinding in the game's options still
works. Where the game itself does not allow running (e.g. while dragging a
body) Hitman walks backwards as before.

### Field of view

`FOV` changes the gameplay camera's horizontal field of view (the game's own
is 67.4°, at every aspect ratio). hm1vk changes the FOV in the engine's camera
object as RenderD3D sets the camera up (`hm1vk/src/fov.rs`), so the engine's
own projection and object culling both use it. Only the normal gameplay camera
is changed: cutscenes, the sniper scope, binoculars and the HUD keep their own
FOV. Wide values show more of the levels than they were built for, e.g. bare
areas past building corners, and Hitman's shoulders can be cut by the camera's
near plane when it is close behind him. The setting needs the Vulkan renderer
(it is ignored with `-d3d`).

## Testing

`tools\test-run.ps1` deploys (through `tools\deploy.ps1`; pass `-Game` for
another game folder or `-NoDeploy` to skip), starts the game, captures screenshots and
collects `hm1vk.log`. `tools\test-scenes.ps1` boots individual missions via
`DefaultScene` (training, Hong Kong, Rotterdam, Budapest, Colombia,
sanitarium, laptop briefing, menus).

## Reverse engineering

`re/` contains the Ghidra headless driver (`ghidra_analyze.bat`,
`ghidra_scripts/ExportAll.java`), the unpacker (`dump_unpacked.ps1`) and
`NOTES.md` with the findings this implementation is based on.

## Known limitations

* The engine's menus are a fixed 640×480 panel and its HUD and subtitles are
  positioned in absolute pixels, so they are small at 3840×2160 unless
  `UIZoom` is raised. When a zoomed menu opens, the game's cursor may start
  outside the visible area; it comes into view at the nearest edge as soon as
  the mouse moves.
* Pre-mission scenes (`*_Pre.zip`) cannot be booted directly with
  `DefaultScene`; the original renderer behaves the same.
* Environment bump mapping (EMBM) is not advertised, so the engine uses its
  non-bump path.
* Point sprites, user clip planes and vertex blending are not used by the game
  and are not implemented.
