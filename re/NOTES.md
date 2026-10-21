# Reverse-engineering notes — Hitman: Codename 47 (Steam build)

Tools: Ghidra 12.1.4 headless (`re/ghidra_analyze.bat`, script
`re/ghidra_scripts/ExportAll.java`), objdump, a runtime memory dump of the
packed executable (`re/dump_unpacked.ps1`).

## Binaries

| File | Role | Notes |
|---|---|---|
| Hitman.Exe | 19 KB launcher | PECompact2-packed, wrapped by Steam DRM ("Failed to find Steam" when started directly). Imports only LoadLibrary/GetProcAddress/VirtualAlloc/VirtualFree before unpacking. |
| Globals.dll | Engine globals | Exports `g_pSysInterface`, `g_pSysCom`, `g_pSysFile`, `g_pSysInput`, `g_pSysMem`, `GlobalEngineVersion`. |
| System.dll | Engine core (Glacier) | Creates the `ZSysInterface` singleton in DllMain; parses the .ini; loads the plug-in DLLs named in it (`DrawDll`, `SoundDll`, `ScriptDll`, `LocaleDll`, `DirectPlayDll`, `RenderVideo.dll`). Exports `RunEngine`, `EditPrint`, C-Dilla descriptors. |
| RenderD3D.dll | Draw plug-in (Direct3D 7) | `Create()` returns a 0x78-byte render object. Imports only `DirectDrawCreateEx` and `DirectDrawEnumerateExA` from DDRAW.dll; everything else is COM (IDirectDraw7, IDirect3D7, IDirect3DDevice7, IDirectDrawSurface7, IDirect3DVertexBuffer7). |
| RenderOpenGL.dll / Render3DFX.dll | Alternative draw plug-ins | Not used by default (`//DrawDll` in Hitman.ini). |
| EngineData.dll, HitmanDlc.dlc, gsc.dll | Geometry/data loaders, game logic (classes `ZGEOM`, `ZCHAROBJ`, ...), script compiler/VM | Unchanged. |
| Sound.dll (+EAX.dll, a3d.dll) | DirectSound plug-in | Unchanged. |
| Locale.dll, xmlparse.dll | Localised strings (expat) | Unchanged. |

## Hitman.Exe, unpacked

Dumped from memory after the stub unpacked itself. Import directory 0x6630,
entry 0x154C (MSVC CRT), WinMain at 0x401040. IAT: 0x406000 =
`g_pSysInterface` (Globals.dll), 0x406008 GetModuleFileNameA, 0x40600C
CreateFileA, 0x406010 GetLastError, 0x406014 CreateMutexA, 0x40601C
CloseHandle, 0x406020 LoadLibraryA, 0x406084 FreeLibrary.

```
WinMain(hInst, hPrev, lpCmdLine, nShow):
    CreateMutexA(NULL, FALSE, "Hitboy")
    if GetLastError() == ERROR_ALREADY_EXISTS: return 0
    if !check_401000(hInst): return 1              ; mov al,1 / ret (protection removed)
    signal(SIGFPE|SIGABRT|SIGILL|SIGSEGV, handler_401010)
    hSys = LoadLibraryA("System.dll"); if !hSys: return 1
    if strlen(lpCmdLine) >= 2: arg = lpCmdLine
    else:
        GetModuleFileNameA(hInst, path, 260); replace last 4 chars with ".ini"
        if CreateFileA(path, GENERIC_READ, 3, 0, OPEN_EXISTING, 0x80, 0) ok:
            CloseHandle; arg = "@" + path
        else: arg = "@main.ini"
    g_pSysInterface->vtbl[0xC8/4](arg)              ; thiscall: run the engine
    FreeLibrary(hSys); return 0

handler_401010(sig):                                ; shut subsystems down on a crash
    for off in (0x38F5, 0x38F9):                    ; unaligned members of ZSysInterface
        p = *(ZSysInterface + off); if p: p->vtbl[0x28/4]()
```

The exe also exports `?CDAPFN0506_WinMain@@3UCDAPFN_PROPERTIES@@A`, a C-Dilla
copy-protection descriptor: `{0xD4F70933, 3, 0x84, &WinMain, 4, 0...}`.
PE flags: not large-address-aware, no NX/ASLR, base 0x400000.

## ZSysInterface fields used here

| Offset | Type | Meaning | Set by |
|---|---|---|---|
| 0x12 | u8 | fullscreen (`Window 0/1` inverted) | System.dll ini parser |
| 0x19 / 0x1D | i32 | width / height | `Resolution WxH` |
| 0x21 / 0x25 | i32 | requested width / height | RenderD3D window setup |
| 0x29 | i32 | colour depth (`ColorDepth`) | ini |
| 0x31 | i32 | refresh rate (`RefreshRate`) | ini |
| 0xA95 / 0xA99 | f32 | half width / height (screen centre) | mode setup |
| 0x38F5 / 0x38F9 | ptr | render / sound subsystem | engine |
| vtbl+0xC8 | method | run engine (arg = "@inifile" or command line) | — |

Other ini keys found in System.dll: Include, DrawDll, SoundDll, ScriptDll,
LocaleDll, DirectPlayDll, PackFile, BackColor, DisableTextures, DisableLight,
EnableTimers, DisablePack, EnableDebugOptions, AutoDumpName, LoadFilter,
DefaultScene, SoundBank, AnimFile, ProjectFile, EnableConsole, StartUpperPos,
ShadowDetail, Anisotropy, LevelOfDetail, TextureResolution, DisableOptions,
EnableEMBM, Antialias, DisablePAL, DisableMultiTexture, EnableTripleBuffering,
DisableHWTnL, DisableDXT, StereoView, ConsoleCmd, ScriptDebug, DisableWBuffer,
TrilinearFiltering, RefreshRate, ErrorLog, NoBlood. `DefaultScene
C0_Training\C0_1.zip` boots straight into a mission (used by the test scripts).

## RenderD3D.dll display-mode logic (why 4K needs a fix)

`FUN_0fba9320` (window setup) snaps the fullscreen width up to the next entry
of a 4:3 table and forces the matching height:
512→384, 640→480, 800→600, 1024→768, 1280→1024, 1600→1200. Wider widths fall
through every case and leave the height uninitialised (it ends up holding the
"Direct3D" title pointer). Mode selection (`0fb81b14..0fb81e55`) then compares
each enumerated mode's `dwWidth`/`dwHeight` with ZSysInterface+0x19/+0x1D and
fails with *"Unable to find a suitable display mode for true color"*.
hm1vk (`hm1vk/src/engine.rs`) restores the .ini resolution in those fields at
the first DirectDraw call, before mode selection.

Device selection: devices are recorded in a 0x4D0-byte table at 0x0FBBEF20
(count at 0x0FBC4F68). The T&L HAL (`{F5049E78-…}`) is preferred over the HAL
(`{84E63DE0-…}`) unless `DisableHWTnL`; modes are filtered per device with
`D3DDEVICEDESC7.dwDeviceRenderBitDepth` (DDBD_16/24/32).

Texture upload: DXT levels are filled with a linear memcpy of the level's
data into `Lock()`ed memory; mip levels are reached with
`GetAttachedSurface(DDSCAPS_TEXTURE|DDSCAPS_MIPMAP|DDSCAPS_COMPLEX)`. 32-bit
textures are created as a system-memory surface plus a managed texture and
copied with `Blt(DDBLT_WAIT)`. Geometry is drawn with
`DrawIndexedPrimitiveVB` from a 10000-vertex `D3DFVF_XYZ|DIFFUSE|TEX3`
(fvf 0x342) vertex buffer; 2D elements with `DrawPrimitive` triangle fans.
