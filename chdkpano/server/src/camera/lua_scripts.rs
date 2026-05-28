//! Lua templates that run on the camera.
//!
//! Kept in one place so future scripts (multi-camera clock sync, AE bracketing,
//! whatever) have an obvious home. Each helper returns the literal Lua source
//! string — the camera-side interpreter parses it via ExecuteScript.

/// One-shot snapshot of every interesting runtime value, piped into a single
/// pipe-delimited string for cheap parsing on the server. One PTP round-trip
/// for ~16 fields.
pub const LIVE_STATE: &str = "\
    local function s(f) local ok, v = pcall(f); if ok then return tostring(v) end return '?' end \
    local m1, m2, m3 = get_mode() \
    return tostring(m1)..'|'..tostring(m2)..'|'..tostring(m3) \
        ..'|'..s(function() return get_zoom() end) \
        ..'|'..s(function() return get_exp_count() end) \
        ..'|'..s(function() return get_vbatt() end) \
        ..'|'..s(function() return get_image_dir() end) \
        ..'|'..s(function() return get_free_disk_space() end) \
        ..'|'..s(function() return get_iso_mode() end) \
        ..'|'..s(function() return get_sv96() end) \
        ..'|'..s(function() return get_tv96() end) \
        ..'|'..s(function() return get_av96() end) \
        ..'|'..s(function() return get_focus() end) \
        ..'|'..s(function() return get_propset() end) \
        ..'|'..s(function() return get_flash_mode() end) \
        ..'|'..s(function() return get_flash_ready() end) \
        ..'|'..s(function() return get_shooting() end)";

/// Directory listing via `os.listdir` + `os.stat`, with fallbacks for
/// builds where the SD root (`A`) is finicky (returns nil unless the path
/// has a trailing slash or `.`).
pub fn list_dir(path: &str) -> String {
    format!(
        "local path = '{path}' \
         if not os.listdir then return 'ERR_NOLIST' end \
         local function try_list(p) \
           local ok, t = pcall(os.listdir, p) \
           if ok and type(t) == 'table' then return t end \
           return nil \
         end \
         local function join(p, name) \
           if p == '' or p == '/' then return name end \
           if string.sub(p, -1) == '/' then return p .. name end \
           return p .. '/' .. name \
         end \
         local t = try_list(path) \
         if not t then t = try_list(path .. '/') end \
         if not t and path == 'A' then t = try_list('A/.') end \
         if not t then return 'ERR_LIST|nil' end \
         local out = {{}} \
         for _, e in ipairs(t) do \
           if e ~= '.' and e ~= '..' then \
             local full = join(path, e) \
             local sok, st = pcall(os.stat, full) \
             local is_dir = (sok and st and st.is_dir) and '1' or '0' \
             local size = (sok and st and st.size) or 0 \
             table.insert(out, e..':'..is_dir..':'..size) \
           end \
         end \
         return table.concat(out, '\\n')"
    )
}

/// Switch the camera into the target mode (1 = record, 0 = play). When
/// switching to record we also force the LCD on so the viewfinder pipeline
/// runs and `get_display_data` can return real frames. The `type()` guards
/// are because not every CHDK build exposes set_lcd_display / set_backlight
/// / request_live_view.
pub fn switch_mode(target: u32) -> String {
    let want_record = target == 1;
    let truthy = if want_record { "true" } else { "false" };
    format!(
        "local in_record = get_mode() \
         if {truthy} ~= (in_record and true or false) then \
           switch_mode_usb({target}) \
           sleep(2000) \
         end \
         if {truthy} then \
           if type(set_lcd_display) == 'function' then set_lcd_display(1) end \
           if type(set_backlight)   == 'function' then set_backlight(1)   end \
           if type(request_live_view) == 'function' then request_live_view(15) end \
         end \
         return get_mode() and 'record' or 'play'"
    )
}

/// Take one photo. Used by the pano array's `shoot_all` — each camera runs
/// this independently; for true simultaneity see `clock_sync_shoot` below.
pub const SHOOT_NOW: &str = "shoot() return 'ok'";

/// Read the camera's monotonic clock (ms since boot) — used by clock-sync
/// shooting to compute the offset between camera time and host time.
pub const READ_CLOCK_MS: &str = "return get_tick_count()";

/// Synchronized shoot. The host computes a future camera-local timestamp
/// (after offset calibration) and passes it as `target_ms`. Each camera
/// busy-waits until its tick matches, then shoots. Replaces the chdkptp
/// "clock sync" trick from the original Lua harness.
pub fn shoot_at_camera_ms(target_ms: i64) -> String {
    format!(
        "local target = {target_ms} \
         while get_tick_count() < target do end \
         shoot() \
         return get_tick_count()"
    )
}
