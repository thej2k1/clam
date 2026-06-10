use windows::core::GUID;
use windows::Win32::Foundation::*;
use windows::Win32::System::Power::*;

extern "system" {
    fn LocalFree(hmem: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
}

use crate::state::SavedState;

// Buttons/Lid subgroup: {4F971E89-EEBD-4455-A8DE-9E59040E7347}
const SUB_BUTTONS: GUID = GUID {
    data1: 0x4F971E89,
    data2: 0xEEBD,
    data3: 0x4455,
    data4: [0xA8, 0xDE, 0x9E, 0x59, 0x04, 0x0E, 0x73, 0x47],
};

// Lid-close action setting: {5CA83367-6E45-459F-A27B-476B1D01C936}
// Values: 0 = Do nothing, 1 = Sleep, 2 = Hibernate, 3 = Shut down
const SETTING_LID_CLOSE: GUID = GUID {
    data1: 0x5CA83367,
    data2: 0x6E45,
    data3: 0x459F,
    data4: [0xA2, 0x7B, 0x47, 0x6B, 0x1D, 0x01, 0xC9, 0x36],
};

// Sleep subgroup: {238C9FA8-0AAD-41ED-83F4-97BE242C8F20}
const SUB_SLEEP: GUID = GUID {
    data1: 0x238C9FA8,
    data2: 0x0AAD,
    data3: 0x41ED,
    data4: [0x83, 0xF4, 0x97, 0xBE, 0x24, 0x2C, 0x8F, 0x20],
};

// Sleep-after / idle standby timeout: {29F6C1DB-86DA-48C5-9FDB-F2B67B1F44DA}
// Value in seconds; 0 = Never
const SETTING_STANDBY_IDLE: GUID = GUID {
    data1: 0x29F6C1DB,
    data2: 0x86DA,
    data3: 0x48C5,
    data4: [0x9F, 0xDB, 0xF2, 0xB6, 0x7B, 0x1F, 0x44, 0xDA],
};

pub const DEFAULT_LID_CLOSE: u32 = 1;
pub const DEFAULT_SLEEP_AC: u32 = 1800;
pub const DEFAULT_SLEEP_DC: u32 = 900;

pub fn is_access_denied(err: u32) -> bool {
    err == ERROR_ACCESS_DENIED.0
}

// PowerGetActiveScheme allocates a GUID buffer; caller must free with LocalFree.
fn get_active_scheme() -> Result<GUID, u32> {
    unsafe {
        let mut ptr: *mut GUID = std::ptr::null_mut();
        let err = PowerGetActiveScheme(None, &mut ptr);
        if err.0 != 0 {
            return Err(err.0);
        }
        let guid = *ptr;
        LocalFree(ptr as *mut _);
        Ok(guid)
    }
}

#[derive(Clone, Copy)]
enum PowerSource {
    AC,
    DC,
}

fn read_power_value(scheme: &GUID, subgroup: &GUID, setting: &GUID, source: PowerSource) -> Result<u32, u32> {
    unsafe {
        let mut val = 0u32;
        let err = match source {
            PowerSource::AC => PowerReadACValueIndex(
                None,
                Some(scheme as *const GUID),
                Some(subgroup as *const GUID),
                Some(setting as *const GUID),
                &mut val,
            )
            .0,
            PowerSource::DC => PowerReadDCValueIndex(
                None,
                Some(scheme as *const GUID),
                Some(subgroup as *const GUID),
                Some(setting as *const GUID),
                &mut val,
            ),
        };
        if err != 0 {
            return Err(err);
        }
        Ok(val)
    }
}

fn write_power_value(
    scheme: &GUID,
    subgroup: &GUID,
    setting: &GUID,
    value: u32,
    source: PowerSource,
) -> Result<(), u32> {
    unsafe {
        let err = match source {
            PowerSource::AC => PowerWriteACValueIndex(
                None,
                scheme,
                Some(subgroup as *const GUID),
                Some(setting as *const GUID),
                value,
            )
            .0,
            PowerSource::DC => PowerWriteDCValueIndex(
                None,
                scheme,
                Some(subgroup as *const GUID),
                Some(setting as *const GUID),
                value,
            ),
        };
        if err != 0 {
            return Err(err);
        }
        Ok(())
    }
}

// PowerSetActiveScheme re-applies the scheme so writes take effect.
fn apply_scheme(scheme: &GUID) -> Result<(), u32> {
    unsafe {
        let err = PowerSetActiveScheme(None, Some(scheme as *const GUID));
        if err.0 != 0 {
            return Err(err.0);
        }
        Ok(())
    }
}

pub fn read_current_settings() -> Result<SavedState, u32> {
    let scheme = get_active_scheme()?;
    Ok(SavedState {
        lid_close_ac: read_power_value(&scheme, &SUB_BUTTONS, &SETTING_LID_CLOSE, PowerSource::AC)?,
        lid_close_dc: read_power_value(&scheme, &SUB_BUTTONS, &SETTING_LID_CLOSE, PowerSource::DC)?,
        sleep_idle_ac: read_power_value(&scheme, &SUB_SLEEP, &SETTING_STANDBY_IDLE, PowerSource::AC)?,
        sleep_idle_dc: read_power_value(&scheme, &SUB_SLEEP, &SETTING_STANDBY_IDLE, PowerSource::DC)?,
    })
}

pub fn write_stay_awake() -> Result<(), u32> {
    let scheme = get_active_scheme()?;
    for src in [PowerSource::AC, PowerSource::DC] {
        write_power_value(&scheme, &SUB_BUTTONS, &SETTING_LID_CLOSE, 0, src)?;
        write_power_value(&scheme, &SUB_SLEEP, &SETTING_STANDBY_IDLE, 0, src)?;
    }
    apply_scheme(&scheme)
}

pub fn write_settings(state: &SavedState) -> Result<(), u32> {
    let scheme = get_active_scheme()?;
    write_power_value(&scheme, &SUB_BUTTONS, &SETTING_LID_CLOSE, state.lid_close_ac, PowerSource::AC)?;
    write_power_value(&scheme, &SUB_BUTTONS, &SETTING_LID_CLOSE, state.lid_close_dc, PowerSource::DC)?;
    write_power_value(&scheme, &SUB_SLEEP, &SETTING_STANDBY_IDLE, state.sleep_idle_ac, PowerSource::AC)?;
    write_power_value(&scheme, &SUB_SLEEP, &SETTING_STANDBY_IDLE, state.sleep_idle_dc, PowerSource::DC)?;
    apply_scheme(&scheme)
}

pub fn write_defaults() -> Result<(), u32> {
    let scheme = get_active_scheme()?;
    write_power_value(&scheme, &SUB_BUTTONS, &SETTING_LID_CLOSE, DEFAULT_LID_CLOSE, PowerSource::AC)?;
    write_power_value(&scheme, &SUB_BUTTONS, &SETTING_LID_CLOSE, DEFAULT_LID_CLOSE, PowerSource::DC)?;
    write_power_value(&scheme, &SUB_SLEEP, &SETTING_STANDBY_IDLE, DEFAULT_SLEEP_AC, PowerSource::AC)?;
    write_power_value(&scheme, &SUB_SLEEP, &SETTING_STANDBY_IDLE, DEFAULT_SLEEP_DC, PowerSource::DC)?;
    apply_scheme(&scheme)
}
