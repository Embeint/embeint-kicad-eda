// SPDX-License-Identifier: GPL-3.0-or-later
//! `KIPLATFORM::WEBVIEW` from `libs/kiplatform/include/kiplatform/webview.h`.
//!
//! The header declares three functions and nothing else. The Linux bodies are in
//! `libs/kiplatform/port/wxgtk/webview.cpp` ([`crate::owners::PORT_WXGTK_UI`]).

use kicad_map_helpers::WxString;

/// The `KIPLATFORM::WEBVIEW` functions, implemented by the native host. A `wxWebView*` is an
/// optional host handle (`None` is a null pointer, which the header documents as invalid).
pub trait WebviewPlatform {
    /// Host handle for a `wxWebView*`.
    type WebView;

    /// `bool SaveCookies( wxWebView* aWebView, const wxString& aTargetFile )`.
    fn save_cookies(&mut self, web_view: Option<&Self::WebView>, target_file: &WxString) -> bool;

    /// `bool LoadCookies( wxWebView* aWebView, const wxString& aSourceFile )`.
    fn load_cookies(&mut self, web_view: Option<&Self::WebView>, source_file: &WxString) -> bool;

    /// `bool DeleteCookies( wxWebView* aWebView )`.
    fn delete_cookies(&mut self, web_view: Option<&Self::WebView>) -> bool;
}
