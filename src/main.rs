#![windows_subsystem = "windows"]

mod autostart;
mod power;
mod state;
mod tray;

use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::System::Threading::CreateMutexW;

fn main() {
    // Single-instance guard: if a mutex with this name already exists, another copy is running.
    let handle = unsafe { CreateMutexW(None, false, w!("Global\\Clam_SingleInstance")) };
    let last_err = unsafe { GetLastError() };
    match handle {
        Ok(h) => {
            if last_err == ERROR_ALREADY_EXISTS {
                unsafe { let _ = CloseHandle(h); }
                return;
            }
        }
        Err(_) => return,
    }

    // Crash/kill recovery: if the saved-state file exists, a previous instance was active
    // when it exited. Restore the captured originals and start in NORMAL state.
    if state::state_file_exists() {
        match state::load_state() {
            Some(saved) => {
                let _ = power::write_settings(&saved);
            }
            None => {
                let _ = power::write_defaults();
            }
        }
        state::clear_state();
    }

    tray::run();
}
