// SPDX-License-Identifier: GPL-3.0-or-later
//! `KIPLATFORM::UI` from `libs/kiplatform/include/kiplatform/ui.h`.
//!
//! Ported from the header: the [`NativeDisplayType`] enumerators and the default argument
//! `int* aDisplayType = nullptr` of `GetNativeWindowHandles`
//! ([`UiPlatform::get_native_window_handles_without_display_type`]). The functions are the
//! host boundary [`UiPlatform`]; their Linux bodies are in
//! `libs/kiplatform/port/wxgtk/ui.cpp` ([`crate::owners::PORT_WXGTK_UI`]).

use kicad_map_helpers::WxString;
pub use kicad_math::vector2wx::{WxPoint, WxSize};

/// `KIPLATFORM::UI::NATIVE_DISPLAY_TYPE`, the display server types that
/// `GetNativeWindowHandles` stores through its `int*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum NativeDisplayType {
    /// `DISPLAY_UNKNOWN = 0`.
    Unknown = 0,
    /// `DISPLAY_X11 = 1`.
    X11 = 1,
    /// `DISPLAY_WAYLAND = 2`.
    Wayland = 2,
    /// `DISPLAY_WIN32 = 3`.
    Win32 = 3,
    /// `DISPLAY_COCOA = 4`.
    Cocoa = 4,
}

impl NativeDisplayType {
    /// The enumerator's value as the `int` it is stored in.
    pub const fn value(self) -> i32 {
        self as i32
    }

    /// The enumerator with value `value`, if there is one.
    pub const fn from_value(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Unknown),
            1 => Some(Self::X11),
            2 => Some(Self::Wayland),
            3 => Some(Self::Win32),
            4 => Some(Self::Cocoa),
            _ => None,
        }
    }
}

/// The `KIPLATFORM::UI` window functions, implemented by the native host.
///
/// wx objects are host handles; a pointer parameter is an `Option` whose `None` is a null
/// pointer. `wxPoint` and `wxSize` are the accepted value co-ports from `kicad_math`.
pub trait UiPlatform {
    /// Host handle for a `wxWindow` (also used for `const wxWindow*`).
    type Window;
    /// Host handle for a `wxNonOwnedWindow`.
    type NonOwnedWindow;
    /// Host handle for a `wxTopLevelWindow`.
    type TopLevelWindow;
    /// Host handle for a `wxChoice`.
    type Choice;
    /// Host handle for a `wxDialog`.
    type Dialog;
    /// Host handle for a `wxDataViewCtrl`.
    type DataViewCtrl;
    /// Host handle for a `wxDataViewColumn`.
    type DataViewColumn;
    /// A `wxColour` value.
    type Colour;
    /// A `wxKeyEvent` value.
    type KeyEvent;
    /// A `wxStockCursor` value.
    type StockCursor;
    /// A native `void*` handle stored by `GetNativeWindowHandles`.
    type NativeHandle;

    /// `bool IsDarkTheme()`.
    fn is_dark_theme(&mut self) -> bool;

    /// `wxColour GetDialogBGColour()`.
    fn get_dialog_bg_colour(&mut self) -> Self::Colour;

    /// `void ForceFocus( wxWindow* aWindow )`.
    fn force_focus(&mut self, window: Option<&Self::Window>);

    /// `bool IsWindowActive( wxWindow* aWindow )`.
    fn is_window_active(&mut self, window: Option<&Self::Window>) -> bool;

    /// `void ReparentModal( wxNonOwnedWindow* aWindow )`.
    fn reparent_modal(&mut self, window: Option<&Self::NonOwnedWindow>);

    /// `void ReparentWindow( wxNonOwnedWindow* aWindow, wxTopLevelWindow* aParent )`.
    fn reparent_window(
        &mut self,
        window: Option<&Self::NonOwnedWindow>,
        parent: Option<&Self::TopLevelWindow>,
    );

    /// `void FixupCancelButtonCmdKeyCollision( wxWindow* aWindow )`.
    fn fixup_cancel_button_cmd_key_collision(&mut self, window: Option<&Self::Window>);

    /// `bool IsStockCursorOk( wxStockCursor aCursor )`.
    fn is_stock_cursor_ok(&mut self, cursor: Self::StockCursor) -> bool;

    /// `void LargeChoiceBoxHack( wxChoice* aChoice )`.
    fn large_choice_box_hack(&mut self, choice: Option<&Self::Choice>);

    /// `void EllipsizeChoiceBox( wxChoice* aChoice )`.
    fn ellipsize_choice_box(&mut self, choice: Option<&Self::Choice>);

    /// `double GetPixelScaleFactor( const wxWindow* aWindow )`.
    fn get_pixel_scale_factor(&mut self, window: Option<&Self::Window>) -> f64;

    /// `double GetContentScaleFactor( const wxWindow* aWindow )`.
    fn get_content_scale_factor(&mut self, window: Option<&Self::Window>) -> f64;

    /// `double GetDisplayRefreshRate( const wxWindow* aWindow )`.
    fn get_display_refresh_rate(&mut self, window: Option<&Self::Window>) -> f64;

    /// `void GetInfoBarColours( wxColour& aFGColour, wxColour& aBGColour )`.
    fn get_info_bar_colours(&mut self, fg_colour: &mut Self::Colour, bg_colour: &mut Self::Colour);

    /// `wxWindow* CreateCanvasOverlay( wxWindow* aCanvas )`.
    fn create_canvas_overlay(&mut self, canvas: Option<&Self::Window>) -> Option<Self::Window>;

    /// `wxSize GetUnobscuredSize( const wxWindow* aWindow )`.
    fn get_unobscured_size(&mut self, window: Option<&Self::Window>) -> WxSize;

    /// `void SetOverlayScrolling( const wxWindow* aWindow, bool overlay )`.
    fn set_overlay_scrolling(&mut self, window: Option<&Self::Window>, overlay: bool);

    /// `bool AllowIconsInMenus()`.
    fn allow_icons_in_menus(&mut self) -> bool;

    /// `bool SupportsKeyState()`.
    fn supports_key_state(&mut self) -> bool;

    /// `wxKeyEvent GetNativePopupDismissalEvent()`.
    fn get_native_popup_dismissal_event(&mut self) -> Self::KeyEvent;

    /// `bool PrepareChoicePopup( wxWindow* aControl )`.
    fn prepare_choice_popup(&mut self, control: Option<&Self::Window>) -> bool;

    /// `wxPoint GetMousePosition()`.
    fn get_mouse_position(&mut self) -> WxPoint;

    /// `bool WarpPointer( wxWindow* aWindow, int aX, int aY )`.
    fn warp_pointer(&mut self, window: Option<&Self::Window>, x: i32, y: i32) -> bool;

    /// `void ImmControl( wxWindow* aWindow, bool aEnable )`.
    fn imm_control(&mut self, window: Option<&Self::Window>, enable: bool);

    /// `void ImeNotifyCancelComposition( wxWindow* aWindow )`.
    fn ime_notify_cancel_composition(&mut self, window: Option<&Self::Window>);

    /// `bool InfiniteDragPrepareWindow( wxWindow* aWindow )`.
    fn infinite_drag_prepare_window(&mut self, window: Option<&Self::Window>) -> bool;

    /// `void InfiniteDragReleaseWindow()`.
    fn infinite_drag_release_window(&mut self);

    /// `void StabilizeWindowPosition( wxWindow* aWindow )`.
    fn stabilize_window_position(&mut self, window: Option<&Self::Window>);

    /// `wxPoint GetRestorablePosition( wxWindow* aWindow )`.
    fn get_restorable_position(&mut self, window: Option<&Self::Window>) -> WxPoint;

    /// `void SetFloatLevel( wxWindow* aWindow )`.
    fn set_float_level(&mut self, window: Option<&Self::Window>);

    /// `void ReleaseChildWindow( wxNonOwnedWindow* aWindow )`.
    fn release_child_window(&mut self, window: Option<&Self::NonOwnedWindow>);

    /// `void AllowNetworkFileSystems( wxDialog* aDialog )`.
    fn allow_network_file_systems(&mut self, dialog: Option<&Self::Dialog>);

    /// `void CancelPendingScroll( wxDataViewCtrl* aCtrl )`.
    fn cancel_pending_scroll(&mut self, ctrl: Option<&Self::DataViewCtrl>);

    /// `int GetColumnWidth( const wxDataViewColumn* aColumn )`.
    fn get_column_width(&mut self, column: Option<&Self::DataViewColumn>) -> i32;

    /// `void SetWMClass( wxWindow* aWindow, const wxString& aClass )`.
    fn set_wm_class(&mut self, window: Option<&Self::Window>, class: &WxString);

    /// `void GetNativeWindowHandles( wxWindow* aWindow, void** aHandle, void** aDisplay,
    /// int* aDisplayType )`. Each out pointer is an `Option` (`None` is null).
    fn get_native_window_handles(
        &mut self,
        window: Option<&Self::Window>,
        handle: Option<&mut Self::NativeHandle>,
        display: Option<&mut Self::NativeHandle>,
        display_type: Option<&mut i32>,
    );

    /// `GetNativeWindowHandles( aWindow, aHandle, aDisplay )`: the default argument
    /// `aDisplayType = nullptr`.
    fn get_native_window_handles_without_display_type(
        &mut self,
        window: Option<&Self::Window>,
        handle: Option<&mut Self::NativeHandle>,
        display: Option<&mut Self::NativeHandle>,
    ) {
        self.get_native_window_handles(window, handle, display, None);
    }

    /// `void PrepareForExternalRendering( wxWindow* aWindow )`.
    fn prepare_for_external_rendering(&mut self, window: Option<&Self::Window>);

    /// `void KeepAboveExternalRendering( wxWindow* aChild )`.
    fn keep_above_external_rendering(&mut self, child: Option<&Self::Window>);

    /// `void RebindOnRealize( wxWindow* aWindow )`.
    fn rebind_on_realize(&mut self, window: Option<&Self::Window>);
}

#[cfg(test)]
mod tests {
    use super::NativeDisplayType;

    #[test]
    fn enumerator_values() {
        let all = [
            NativeDisplayType::Unknown,
            NativeDisplayType::X11,
            NativeDisplayType::Wayland,
            NativeDisplayType::Win32,
            NativeDisplayType::Cocoa,
        ];
        for (value, display) in (0..).zip(all) {
            assert_eq!(display.value(), value);
            assert_eq!(NativeDisplayType::from_value(value), Some(display));
        }
        assert_eq!(NativeDisplayType::from_value(5), None);
        assert_eq!(NativeDisplayType::from_value(-1), None);
    }
}
