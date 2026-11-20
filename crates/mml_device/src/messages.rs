/// GRBL 1.1 error codes, worded for the user rather than the developer.
pub fn error_message(code: u32) -> &'static str {
    match code {
        1 => "G-code words consist of a letter and a value; the letter was not found.",
        2 => "Missing or malformed number in a G-code word.",
        3 => "'$' system command not recognised or supported.",
        4 => "Negative value received for a value that must be positive.",
        5 => "Homing is not enabled in the machine settings ($22).",
        6 => "Step pulse time must be at least 3 microseconds.",
        7 => "Settings could not be read from EEPROM; defaults restored.",
        8 => "'$' command only allowed while the machine is idle.",
        9 => "G-code locked out during alarm or jog. Unlock or home first.",
        10 => "Soft limits require homing to be enabled.",
        11 => "Line too long; it exceeds the controller buffer.",
        12 => "Step rate too high for the controller.",
        13 => "Safety door opened.",
        14 => "Startup line too long.",
        15 => "Jog target exceeds machine travel; command ignored.",
        16 => "Invalid jog command.",
        17 => "Laser mode requires PWM output.",
        20 => "Unsupported or invalid G-code command.",
        21 => "Two G-code commands of the same modal group on one line.",
        22 => "Feed rate has not been set.",
        23 => "This G-code command requires an integer value.",
        24 => "Two G-code commands that both use axis words on one line.",
        25 => "Repeated G-code word on one line.",
        26 => "Motion command without axis word.",
        27 => "Invalid line number.",
        28 => "G-code command is missing a required value.",
        29 => "G59.x work coordinate systems are not supported.",
        30 => "G53 is only allowed with G0 and G1.",
        31 => "Axis words found where no command uses them.",
        32 => "G2/G3 arcs need at least one axis word in the plane.",
        33 => "Motion target is invalid.",
        34 => "Arc radius value is invalid.",
        35 => "G2/G3 arcs need at least one offset word in the plane.",
        36 => "Unused value words found on the line.",
        37 => "G43.1 tool length offset is not on the configured axis.",
        38 => "Tool number greater than the maximum supported.",
        _ => "Unknown controller error.",
    }
}

pub fn alarm_message(code: u32) -> &'static str {
    match code {
        1 => "Hard limit triggered. Position is lost: home the machine again.",
        2 => {
            "Motion target exceeds machine travel (soft limit). Position kept; unlock to continue."
        }
        3 => "Reset while moving. Position may be lost: home the machine again.",
        4 => "Probe was already triggered before probing. Check the probe wiring.",
        5 => "Probe did not touch within the programmed distance.",
        6 => "Homing reset by the user.",
        7 => "Safety door opened during homing.",
        8 => "Homing failed to clear the limit switch. Check pull-off and wiring.",
        9 => "Homing could not find the limit switch within the search distance.",
        10 => "Homing failed on the second axis of a dual-motor gantry.",
        _ => "Unknown alarm.",
    }
}

/// Halt reasons of the Makera Carvera (`H:` field of its status).
pub fn halt_message(code: u32) -> &'static str {
    match code {
        1 => "Stopped by hand. Unlock to continue.",
        2 => "Homing failed. Check that nothing blocks the axes, then home again.",
        3 => "Probe did not touch. Check the probe and the distance to the part.",
        4 => "Tool length calibration failed. Check the tool and the setter.",
        5 => "Tool changer failed to home.",
        6 => "Tool number not available in the tool changer (slots 1 to 6, probe 0, laser 8888).",
        7 => "Tool changer could not drop the tool. Check the collet and the empty slot.",
        8 => "Tool changer slot already occupied. Empty the slot of the tool in the spindle.",
        9 => "Spindle too hot. Let it cool down.",
        10 => "Soft limit: the move goes beyond the machine travel.",
        11 => "Cover opened while running.",
        12 => "Wireless probe not answering or not paired.",
        13 => "Emergency stop button pressed.",
        14 => "Electronics too hot.",
        16 => "3D probe crash detected.",
        21 => "Hard limit triggered: reset needed.",
        22..=24 => "Axis motor error: reset needed.",
        25 => "Spindle stall: reset needed.",
        26 => "SD card read failure: reset needed.",
        41 => "Spindle alarm: power the machine off and on.",
        _ => "Unknown halt reason.",
    }
}
