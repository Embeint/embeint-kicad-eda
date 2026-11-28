// SPDX-License-Identifier: GPL-3.0-or-later
//! `KIPLATFORM::UI` touchpad gestures from `libs/kiplatform/include/kiplatform/touchpad.h`.
//!
//! Ported from the header: [`TouchpadGesture`], the handler base class with its virtual
//! defaulted destructor ([`TouchpadGestureHandler`]) and the callback type
//! ([`TouchpadGestureCallback`]). The two functions are the host boundary
//! [`TouchpadPlatform`]; their Linux bodies are in `libs/kiplatform/port/wxgtk/touchpad.cpp`
//! ([`crate::owners::PORT_WXGTK_UI`]).

use alloc::boxed::Box;
pub use kicad_math::vector2wx::WxPoint;

/// `KIPLATFORM::UI::TOUCHPAD_GESTURE`: `double panX, panY, zoomFactor; wxPoint zoomAnchor;`.
///
/// [`Default`] is value-initialization, `TOUCHPAD_GESTURE{}`: the doubles are zero and
/// `zoomAnchor` is `wxPoint()`, `(0, 0)`. Default-initialization (`TOUCHPAD_GESTURE g;`)
/// leaves the doubles indeterminate, which has no Rust equivalent and is not modelled.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TouchpadGesture {
    /// `double panX`.
    pub pan_x: f64,
    /// `double panY`.
    pub pan_y: f64,
    /// `double zoomFactor`.
    pub zoom_factor: f64,
    /// `wxPoint zoomAnchor`.
    pub zoom_anchor: WxPoint,
}

/// `KIPLATFORM::UI::TOUCHPAD_GESTURE_HANDLER`: a polymorphic owner of a platform touchpad
/// registration with `virtual ~TOUCHPAD_GESTURE_HANDLER() = default;`.
///
/// The defaulted virtual destructor means destroying a handler through the base pointer
/// (`std::unique_ptr<TOUCHPAD_GESTURE_HANDLER>`) runs the derived destructor and does
/// nothing of its own. A `Box<dyn TouchpadGestureHandler>` behaves the same: dropping it
/// runs the implementor's `Drop` and the trait adds nothing.
pub trait TouchpadGestureHandler {}

/// The `std::bad_function_call` thrown by calling an empty `std::function`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BadFunctionCall;

/// The target a [`TouchpadGestureCallback`] holds.
pub type GestureTarget = Box<dyn FnMut(&TouchpadGesture)>;

/// `using TOUCHPAD_GESTURE_CALLBACK = std::function<void( const TOUCHPAD_GESTURE& )>`.
#[derive(Default)]
pub struct TouchpadGestureCallback {
    target: Option<GestureTarget>,
}

impl TouchpadGestureCallback {
    /// An empty function (`TOUCHPAD_GESTURE_CALLBACK()` or `nullptr`).
    pub fn empty() -> Self {
        Self { target: None }
    }

    /// A function holding `target`.
    pub fn new(target: impl FnMut(&TouchpadGesture) + 'static) -> Self {
        Self {
            target: Some(Box::new(target)),
        }
    }

    /// `explicit operator bool() const`: whether a target is held.
    pub fn is_set(&self) -> bool {
        self.target.is_some()
    }

    /// `operator()( const TOUCHPAD_GESTURE& )`: call the target with a reference to
    /// `gesture`, or fail with `std::bad_function_call` when empty.
    pub fn call(&mut self, gesture: &TouchpadGesture) -> Result<(), BadFunctionCall> {
        match self.target.as_mut() {
            Some(target) => {
                target(gesture);
                Ok(())
            }
            None => Err(BadFunctionCall),
        }
    }
}

impl core::fmt::Debug for TouchpadGestureCallback {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("TouchpadGestureCallback")
            .field("is_set", &self.is_set())
            .finish()
    }
}

/// The touchpad functions of `KIPLATFORM::UI`, implemented by the native host.
pub trait TouchpadPlatform {
    /// Host handle for a `wxWindow*`.
    type Window;

    /// `bool IsNativeTouchpadGestureAvailable()`.
    fn is_native_touchpad_gesture_available(&mut self) -> bool;

    /// `std::unique_ptr<TOUCHPAD_GESTURE_HANDLER> CreateTouchpadGestureHandler(
    /// wxWindow* aInputWindow, TOUCHPAD_GESTURE_CALLBACK aCallback )`; `None` is a null
    /// window or a null result.
    fn create_touchpad_gesture_handler(
        &mut self,
        input_window: Option<&Self::Window>,
        callback: TouchpadGestureCallback,
    ) -> Option<Box<dyn TouchpadGestureHandler>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::rc::Rc;
    use alloc::vec::Vec;
    use core::cell::RefCell;

    #[test]
    fn value_initialized_gesture_is_zero() {
        let gesture = TouchpadGesture::default();
        assert_eq!(
            (
                gesture.pan_x.to_bits(),
                gesture.pan_y.to_bits(),
                gesture.zoom_factor.to_bits()
            ),
            (0, 0, 0)
        );
        assert_eq!(gesture.zoom_anchor, WxPoint::new(0, 0));
    }

    #[test]
    fn callback_forwards_or_reports_bad_call() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&seen);
        let mut callback = TouchpadGestureCallback::new(move |g: &TouchpadGesture| {
            sink.borrow_mut().push(*g);
        });
        let gesture = TouchpadGesture {
            pan_x: -0.0,
            pan_y: f64::NAN,
            zoom_factor: 2.0,
            zoom_anchor: WxPoint::new(i32::MIN, 7),
        };
        assert_eq!(callback.call(&gesture), Ok(()));
        assert_eq!(seen.borrow().len(), 1);
        assert_eq!(
            TouchpadGestureCallback::empty().call(&gesture),
            Err(BadFunctionCall)
        );
    }

    #[test]
    fn handler_drop_runs_the_implementor() {
        struct Recorder(Rc<RefCell<u32>>);
        impl TouchpadGestureHandler for Recorder {}
        impl Drop for Recorder {
            fn drop(&mut self) {
                *self.0.borrow_mut() += 1;
            }
        }
        let count = Rc::new(RefCell::new(0));
        let handler: Box<dyn TouchpadGestureHandler> = Box::new(Recorder(Rc::clone(&count)));
        drop(handler);
        assert_eq!(*count.borrow(), 1);
    }
}
