//! Modern run controls (`ModernRun` in hm1vk.ini).
//!
//! The game reads its controls as named actions (`WalkForward`, `Run`, ...)
//! through the input manager in System.dll: `ZSysInterface+0x37B1` points at
//! it, and vtable slot 0x24 is `IsActionActive(name, callerId)`. The player
//! (HitmanDlc.dlc) turns those into movement bits each frame:
//!
//! | action          | bits            |
//! |-----------------|-----------------|
//! | `Run`           | run             |
//! | `WalkForward`   | walk            |
//! | `RunBackward`   | run + backward  |
//! | `WalkBackward`  | walk + backward |
//! | `WalkLeft/Right`| strafe          |
//!
//! and picks the movement: *run* without *walk* plays `Run` (forward), with a
//! strafe `Run_Str_Left/Right` (a diagonal run), with *backward*
//! `Run_Backward`; as soon as *walk* is set it walks. So stock controls run
//! only forwards (Shift alone runs, Shift+S still walks backwards).
//!
//! With `ModernRun = 1` the run key modifies the direction keys instead, as in
//! a modern shooter: hm1vk hooks `IsActionActive` and answers the movement
//! actions from the keys that are held:
//!
//! * Shift + W (+ A/D)  -> `Run` (+ strafe): run forwards / diagonally
//! * Shift + S (+ A/D)  -> `RunBackward` (+ strafe): run backwards / diagonally
//! * Shift alone        -> nothing (stand still)
//! * Shift + A/D only   -> strafe at walking speed (the game has no sideways run)
//!
//! Every other action is passed through unchanged, with the original caller
//! id, so "pressed once" actions keep working.

use std::ffi::{c_char, c_void, CStr};

use windows_sys::Win32::System::LibraryLoader::GetModuleHandleA;
use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_READWRITE};

/// Offset of the input manager pointer in ZSysInterface.
const INPUT_MANAGER_OFFSET: usize = 0x37B1;
/// Vtable slot of IsActionActive and its RVA in System.dll (Steam build).
const IS_ACTION_SLOT: usize = 0x24;
const IS_ACTION_RVA: usize = 0x2350;

type IsAction = unsafe extern "thiscall" fn(this: *mut c_void, name: *const c_char, caller: u32) -> u32;

static mut ORIGINAL: Option<IsAction> = None;
static mut LAST_KEYS: u8 = 0;
static mut STATE: u8 = 0; // 0 = not tried, 1 = installed, 2 = failed

// Thiscall thunk: passes this, name, caller id and the caller's return address
// to `action_hook` (cdecl) and returns with `ret 8` like the original.
core::arch::global_asm!(
    ".globl _hm1vk_action_thunk",
    "_hm1vk_action_thunk:",
    "    mov eax, dword ptr [esp]",
    "    push eax",
    "    push dword ptr [esp + 12]",
    "    push dword ptr [esp + 12]",
    "    push ecx",
    "    call {hook}",
    "    add esp, 16",
    "    ret 8",
    hook = sym action_hook,
);

extern "C" {
    fn hm1vk_action_thunk();
}

/// The player's controls function in HitmanDlc.dlc (Steam build): vtable
/// slot RVA and function RVA. It reads the movement actions; while it runs,
/// `PLAYER` points at the player so the hook can check whether running is
/// allowed.
const PLAYER_INPUT_SLOT_RVA: usize = 0x20A70C;
const PLAYER_INPUT_RVA: usize = 0x130C70;
/// Player byte that turns Run into walking (checked by the game's own code).
const PLAYER_NO_RUN_OFFSET: usize = 0x77A;

type PlayerInput = unsafe extern "thiscall" fn(this: *mut u8);
static mut PLAYER_INPUT: Option<PlayerInput> = None;
static mut PLAYER: *mut u8 = std::ptr::null_mut();

unsafe extern "thiscall" fn player_input_hook(this: *mut u8) {
    PLAYER = this;
    if let Some(f) = PLAYER_INPUT {
        f(this);
    }
    PLAYER = std::ptr::null_mut();
}

unsafe fn base_dlc() -> usize {
    GetModuleHandleA(b"HitmanDlc.dlc\0".as_ptr()) as usize
}

unsafe fn install_player_hook(base: usize) {
    if base == 0 {
        return;
    }
    let slot = (base + PLAYER_INPUT_SLOT_RVA) as *mut usize;
    if slot.read() != base + PLAYER_INPUT_RVA {
        crate::info!("ModernRun: unknown HitmanDlc.dlc build; running backwards is not blocked while dragging");
        return;
    }
    PLAYER_INPUT = Some(std::mem::transmute::<usize, PlayerInput>(slot.read()));
    let mut old = 0u32;
    if VirtualProtect(slot as *const c_void, 4, PAGE_READWRITE, &mut old) != 0 {
        slot.write(player_input_hook as *const () as usize);
        VirtualProtect(slot as *const c_void, 4, old, &mut old);
    }
}

/// Ids for the hook's own queries (any non-zero value; movement actions are
/// plain "held" states, which do not depend on the caller).
const OWN_ID: u32 = 0x484D_564B;

unsafe fn orig(this: *mut c_void, name: &[u8]) -> bool {
    match ORIGINAL {
        Some(f) => f(this, name.as_ptr() as *const c_char, OWN_ID) as u8 != 0,
        None => false,
    }
}

extern "C" fn action_hook(this: *mut c_void, name: *const c_char, caller: u32, ret: u32) -> u32 {
    unsafe {
        let Some(f) = ORIGINAL else { return 0 };
        let caller = if caller == 0 { ret } else { caller };
        if !name.is_null() {
            let n = CStr::from_ptr(name).to_bytes();
            let which = if n.eq_ignore_ascii_case(b"Run") {
                1
            } else if n.eq_ignore_ascii_case(b"WalkForward") {
                2
            } else if n.eq_ignore_ascii_case(b"RunBackward") {
                3
            } else if n.eq_ignore_ascii_case(b"WalkBackward") {
                4
            } else {
                0
            };
            if which != 0 {
                let run = orig(this, b"Run\0");
                let fwd = orig(this, b"WalkForward\0");
                let back = orig(this, b"WalkBackward\0");
                let run_back = orig(this, b"RunBackward\0");
                let keys = (run as u8) | (fwd as u8) << 1 | (back as u8) << 2 | (run_back as u8) << 3;
                if keys != LAST_KEYS {
                    LAST_KEYS = keys;
                    crate::debug!("ModernRun: run={run} forward={fwd} backward={back} runbackward={run_back}");
                }
                // Where the game does not allow running (player flag 0x77A,
                // e.g. while dragging a body) it walks; keep walking backwards
                // there too instead of running.
                let player = PLAYER;
                let can_run = player.is_null() || *player.add(PLAYER_NO_RUN_OFFSET) == 0;
                let r = match which {
                    1 => run && fwd && !back,
                    2 => fwd && !run,
                    3 => run_back || (can_run && run && back && !fwd),
                    _ => back && !run_back && !(can_run && run && !fwd),
                };
                return r as u32;
            }
        }
        f(this, name, caller)
    }
}

/// Installs the hook once the engine's input manager exists. Called every
/// frame until it has been tried.
pub unsafe fn install() {
    if STATE != 0 || !crate::state::config().modern_run {
        return;
    }
    let sys = crate::engine::sys_interface();
    if sys.is_null() {
        return;
    }
    let mgr = (sys.add(INPUT_MANAGER_OFFSET) as *const *mut c_void).read_unaligned();
    if mgr.is_null() {
        return;
    }
    STATE = 2;
    let base = GetModuleHandleA(b"System.dll\0".as_ptr()) as usize;
    let vtbl = *(mgr as *const *mut usize);
    let slot = (vtbl as usize + IS_ACTION_SLOT) as *mut usize;
    if base == 0 || slot.read() != base + IS_ACTION_RVA {
        crate::info!("ModernRun: unknown System.dll build; run controls unchanged");
        return;
    }
    ORIGINAL = Some(std::mem::transmute::<usize, IsAction>(slot.read()));
    let mut old = 0u32;
    if VirtualProtect(slot as *const c_void, 4, PAGE_READWRITE, &mut old) == 0 {
        crate::info!("ModernRun: cannot patch the input manager");
        return;
    }
    slot.write(hm1vk_action_thunk as *const () as usize);
    VirtualProtect(slot as *const c_void, 4, old, &mut old);
    STATE = 1;
    install_player_hook(base_dlc());
    crate::info!("ModernRun: run key works with every direction (Shift+W/S, with A/D for diagonals)");
}

