// SPDX-License-Identifier: GPL-3.0-or-later
//! Recording host for the default-argument checks: the counterpart of the probe bodies in
//! `port/oracle/reference/kiplatform_include_probe.cpp`. The six functions with a defaulted
//! parameter record what they receive; every other trait method is never called by the
//! adapter and only returns a neutral value.

use kicad_kiplatform::io::{
    FileLock, FileLockState, IoPlatform, MappedFile, RuntimeError, TargetAttrs,
};
use kicad_kiplatform::secrets::{GetDone, SecretsPlatform, StoreDone};
use kicad_kiplatform::ui::{UiPlatform, WxPoint, WxSize};
use kicad_map_helpers::{WChar, WxString};

/// What a recorded call received.
#[derive(Debug, Default)]
pub struct Recorder {
    /// `wxString` arguments, in order, as units.
    pub strings: Vec<Vec<WChar>>,
    /// The bytes `AtomicWriteFile` received.
    pub data: Option<Vec<u8>>,
    /// Pointer and function checks, in the order the probe makes them.
    pub flags: Vec<(&'static str, bool)>,
    /// What a `bool` function returns.
    pub returned: bool,
}

impl Recorder {
    fn record(&mut self, strings: &[&WxString]) {
        self.strings = strings.iter().map(|s| s.units().to_vec()).collect();
    }
}

impl IoPlatform for Recorder {
    type File = ();
    type FileName = ();

    fn seq_f_open(&mut self, _: &WxString, _: &WxString) -> Option<()> {
        None
    }
    fn duplicate_permissions(&mut self, _: &WxString, _: &WxString) -> bool {
        false
    }
    fn make_writeable(&mut self, _: &WxString) -> bool {
        false
    }
    fn is_file_hidden(&mut self, _: &WxString) -> bool {
        false
    }
    fn long_path_adjustment(&mut self, _: &mut ()) {}
    fn timestamp_dir(&mut self, _: &WxString, _: &WxString) -> i64 {
        0
    }
    fn flush_to_disk(&mut self, _: Option<&mut ()>) -> bool {
        false
    }
    fn flush_directory(&mut self, _: &WxString) -> bool {
        false
    }

    fn atomic_rename(
        &mut self,
        src: &WxString,
        dst: &WxString,
        error: Option<&mut WxString>,
    ) -> bool {
        self.record(&[src, dst]);
        self.flags.push(("error_is_null", error.is_none()));
        self.returned
    }

    fn make_sibling_temp_path(&mut self, _: &WxString) -> WxString {
        WxString::default()
    }

    fn open_unique_sibling_temp_file(
        &mut self,
        target_path: &WxString,
        mode: &WxString,
        temp_path_out: Option<&mut WxString>,
        error: Option<&mut WxString>,
    ) -> Option<()> {
        self.record(&[target_path, mode]);
        self.flags
            .push(("temp_path_out_is_null", temp_path_out.is_none()));
        self.flags.push(("error_is_null", error.is_none()));
        None
    }

    fn resolve_symlink_target(&mut self, _: &WxString) -> WxString {
        WxString::default()
    }
    fn capture_target_attributes(&mut self, _: &WxString) -> TargetAttrs {
        TargetAttrs::default()
    }
    fn apply_target_attributes(&mut self, _: &WxString, _: &TargetAttrs) -> bool {
        false
    }

    fn commit_temp_file(
        &mut self,
        temp_path: &WxString,
        target_path: &WxString,
        error: Option<&mut WxString>,
    ) -> bool {
        self.record(&[temp_path, target_path]);
        self.flags.push(("error_is_null", error.is_none()));
        self.returned
    }

    fn atomic_write_file(
        &mut self,
        target_path: &WxString,
        data: &[u8],
        error: Option<&mut WxString>,
    ) -> bool {
        self.record(&[target_path]);
        self.data = Some(data.to_vec());
        self.flags.push(("error_is_null", error.is_none()));
        self.returned
    }

    fn mapped_file_open(&mut self, _: &WxString) -> Result<MappedFile, RuntimeError> {
        Err(RuntimeError {
            what: b"not used by the adapter".to_vec(),
        })
    }
    fn mapped_file_close(&mut self, _: MappedFile) {}
    fn file_lock_acquire(&mut self, _: &mut FileLock, _: &WxString, _: &mut bool) -> FileLockState {
        FileLockState::None
    }
    fn file_lock_open_for_inspect(&mut self, _: &mut FileLock, _: &WxString, _: &mut bool) -> bool {
        false
    }
    fn file_lock_is_open(&self, _: &FileLock) -> bool {
        false
    }
    fn file_lock_read_all(&self, _: &FileLock, _: &mut Vec<u8>) -> bool {
        false
    }
    fn file_lock_rewrite(&mut self, _: &mut FileLock, _: &[u8]) -> bool {
        false
    }
    fn file_lock_release(&mut self, _: &mut FileLock) {}
    fn file_lock_destroy(&mut self, _: FileLock) {}
    fn file_lock_move_construct(&mut self, _: &mut FileLock) -> FileLock {
        FileLock::new()
    }
    fn file_lock_move_assign(&mut self, _: &mut FileLock, _: &mut FileLock) {}
}

impl SecretsPlatform for Recorder {
    fn store_secret(&mut self, _: &WxString, _: &WxString, _: &WxString) -> bool {
        false
    }

    fn store_secret_async(
        &mut self,
        service: &WxString,
        key: &WxString,
        secret: &WxString,
        done: StoreDone,
    ) {
        self.record(&[service, key, secret]);
        self.flags.push(("done_is_empty", done.is_none()));
    }

    fn get_secret(&mut self, _: &WxString, _: &WxString, _: &mut WxString) -> bool {
        false
    }
    fn get_secret_async(&mut self, _: &WxString, _: &WxString, _: GetDone) {}
    fn get_secret_no_prompt(&mut self, _: &WxString, _: &WxString, _: &mut WxString) -> bool {
        false
    }
    fn delete_secret(&mut self, _: &WxString, _: &WxString) -> bool {
        false
    }
}

impl UiPlatform for Recorder {
    type Window = ();
    type NonOwnedWindow = ();
    type TopLevelWindow = ();
    type Choice = ();
    type Dialog = ();
    type DataViewCtrl = ();
    type DataViewColumn = ();
    type Colour = ();
    type KeyEvent = ();
    type StockCursor = ();
    type NativeHandle = ();

    fn is_dark_theme(&mut self) -> bool {
        false
    }
    fn get_dialog_bg_colour(&mut self) {}
    fn force_focus(&mut self, _: Option<&()>) {}
    fn is_window_active(&mut self, _: Option<&()>) -> bool {
        false
    }
    fn reparent_modal(&mut self, _: Option<&()>) {}
    fn reparent_window(&mut self, _: Option<&()>, _: Option<&()>) {}
    fn fixup_cancel_button_cmd_key_collision(&mut self, _: Option<&()>) {}
    fn is_stock_cursor_ok(&mut self, _: ()) -> bool {
        false
    }
    fn large_choice_box_hack(&mut self, _: Option<&()>) {}
    fn ellipsize_choice_box(&mut self, _: Option<&()>) {}
    fn get_pixel_scale_factor(&mut self, _: Option<&()>) -> f64 {
        1.0
    }
    fn get_content_scale_factor(&mut self, _: Option<&()>) -> f64 {
        1.0
    }
    fn get_display_refresh_rate(&mut self, _: Option<&()>) -> f64 {
        0.0
    }
    fn get_info_bar_colours(&mut self, _: &mut (), _: &mut ()) {}
    fn create_canvas_overlay(&mut self, _: Option<&()>) -> Option<()> {
        None
    }
    fn get_unobscured_size(&mut self, _: Option<&()>) -> WxSize {
        WxSize::default()
    }
    fn set_overlay_scrolling(&mut self, _: Option<&()>, _: bool) {}
    fn allow_icons_in_menus(&mut self) -> bool {
        false
    }
    fn supports_key_state(&mut self) -> bool {
        false
    }
    fn get_native_popup_dismissal_event(&mut self) {}
    fn prepare_choice_popup(&mut self, _: Option<&()>) -> bool {
        false
    }
    fn get_mouse_position(&mut self) -> WxPoint {
        WxPoint::default()
    }
    fn warp_pointer(&mut self, _: Option<&()>, _: i32, _: i32) -> bool {
        false
    }
    fn imm_control(&mut self, _: Option<&()>, _: bool) {}
    fn ime_notify_cancel_composition(&mut self, _: Option<&()>) {}
    fn infinite_drag_prepare_window(&mut self, _: Option<&()>) -> bool {
        false
    }
    fn infinite_drag_release_window(&mut self) {}
    fn stabilize_window_position(&mut self, _: Option<&()>) {}
    fn get_restorable_position(&mut self, _: Option<&()>) -> WxPoint {
        WxPoint::default()
    }
    fn set_float_level(&mut self, _: Option<&()>) {}
    fn release_child_window(&mut self, _: Option<&()>) {}
    fn allow_network_file_systems(&mut self, _: Option<&()>) {}
    fn cancel_pending_scroll(&mut self, _: Option<&()>) {}
    fn get_column_width(&mut self, _: Option<&()>) -> i32 {
        0
    }
    fn set_wm_class(&mut self, _: Option<&()>, _: &WxString) {}

    fn get_native_window_handles(
        &mut self,
        window: Option<&()>,
        handle: Option<&mut ()>,
        display: Option<&mut ()>,
        display_type: Option<&mut i32>,
    ) {
        self.flags.push(("window_is_null", window.is_none()));
        self.flags.push(("handle_is_null", handle.is_none()));
        self.flags.push(("display_is_null", display.is_none()));
        self.flags
            .push(("display_type_is_null", display_type.is_none()));
    }

    fn prepare_for_external_rendering(&mut self, _: Option<&()>) {}
    fn keep_above_external_rendering(&mut self, _: Option<&()>) {}
    fn rebind_on_realize(&mut self, _: Option<&()>) {}
}
