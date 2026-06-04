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

/// Read the camera's monotonic tick (`get_tick_count`) — used by clock-sync
/// calibration to compute the offset between camera time and host time.
pub const READ_CLOCK_MS: &str = "return get_tick_count()";

/// The combined per-camera clock-sync script: warmup → busy-wait → fire, all
/// in ONE `lua_State`. This is a faithful port of `shoot_all_clocksync.rs`
/// in chdkptp_rs.
///
/// The single-script constraint is load-bearing: CHDK auto-releases held keys
/// when the `lua_State` is destroyed, so splitting warmup and fire across two
/// `ExecuteScript` calls would silently drop the half-press. Keeping the
/// half-press held across the busy-wait is what makes the shutter fire the
/// instant the spin loop exits.
///
/// `flash` forces the flash on (`FLASH_MODE` 1) when true, or off (2) when
/// false — the multi-camera shot wants a deterministic flash state, not auto.
///
/// Returns nine comma-separated numbers plus the image directory:
///   `t_start,warmup_done,t_exit,t_done,exp_at_start,exp_after_mode,exp_after_half,exp_before_fire,exp_after,<image_dir>`
/// The five `exp_count` checkpoints let the host detect stray actuations, and
/// `image_dir` + `exp_after` together name the file this shot wrote
/// (`<image_dir>/IMG_<exp_after>.JPG`) — no SD-card scan needed.
pub fn clocksync_combined(target_tick: i64, flash: bool) -> String {
    // CHDK FLASH_MODE propcase: 0 = auto, 1 = on, 2 = off.
    let flash_mode = if flash { 1 } else { 2 };
    format!(
        "local t_start = get_tick_count() \
         local exp_at_start = get_exp_count() \
         if not get_mode() then \
           switch_mode_usb(1) \
           sleep(3500) \
         end \
         local exp_after_mode = get_exp_count() \
         local ok, p = pcall(require, 'propcase') \
         if ok then \
           if p.FLASH_MODE then set_prop(p.FLASH_MODE, {flash_mode}) end \
           if p.WB_MODE    then set_prop(p.WB_MODE, 1)    end \
           if p.DRIVE_MODE then set_prop(p.DRIVE_MODE, 0) end \
         end \
         if type(set_iso_mode)    == 'function' then set_iso_mode(1)     end \
         if type(set_sv96)        == 'function' then set_sv96(411)        end \
         if type(set_tv96_direct) == 'function' then set_tv96_direct(576) end \
         press('shoot_half') \
         local af_start = get_tick_count() \
         while not get_shooting() and (get_tick_count() - af_start) < 5000 do \
           sleep(50) \
         end \
         sleep(200) \
         local warmup_done = get_tick_count() \
         local exp_after_half = get_exp_count() \
         local target = {target_tick} \
         while get_tick_count() < target do end \
         local t_exit = get_tick_count() \
         local exp_before_fire = get_exp_count() \
         press('shoot_full') \
         sleep(150) \
         release('shoot_full') \
         release('shoot_half') \
         local t_done = get_tick_count() \
         sleep(1800) \
         local exp_after = get_exp_count() \
         local imgdir = get_image_dir() or '' \
         return t_start..','..warmup_done..','..t_exit..','..t_done..','..exp_at_start..','..exp_after_mode..','..exp_after_half..','..exp_before_fire..','..exp_after..','..imgdir"
    )
}
