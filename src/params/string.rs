// `RefCell<std::string::String>` cannot be shared between threads safely
// within `RjvParams`, the trait `Sync` is not implemented for `RefCell<std::string::String>`
// (if you want to do aliasing and mutation between multiple threads, use `std::sync::RwLock` insteadrustcClick for full compiler diagnostic)
//
// ===

//! Stepped integer parameters.

// use atomic_float::AtomicF32;
use std::fmt::{Debug, Display};
// use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};

use super::internals::ParamPtr;
use super::{Param, ParamFlags, ParamMut};

/// A discrete integer parameter that's stored unnormalized. The range is used for the normalization
/// process.
pub struct StringParam {
    /// The field's current plain value, after monophonic modulation has been applied.
    value: Arc<Mutex<String>>,

    default: String,

    /// Flags to control the parameter's behavior. See [`ParamFlags`].
    flags: ParamFlags,
    /// Optional callback for listening to value changes. The argument passed to this function is
    /// the parameter's new **plain** value. This should not do anything expensive as it may be
    /// called multiple times in rapid succession.
    ///
    /// To use this, you'll probably want to store an `Arc<Atomic*>` alongside the parameter in the
    /// parameters struct, move a clone of that `Arc` into this closure, and then modify that.
    ///
    /// TODO: We probably also want to pass the old value to this function.
    value_changed: Option<Arc<dyn Fn(String) + Send + Sync>>,

    /// The parameter's human readable display name.
    name: String,
    /// The parameter value's unit, added after `value_to_string` if that is set. NIH-plug will not
    /// automatically add a space before the unit.
    unit: &'static str,

    /// Optional custom conversion function from a plain **unnormalized** value to a string.
    value_to_string: Option<Arc<dyn Fn(String) -> String + Send + Sync>>,
    /// Optional custom conversion function from a string to a plain **unnormalized** value. If the
    /// string cannot be parsed, then this should return a `None`. If this happens while the
    /// parameter is being updated then the update will be canceled.
    ///
    /// The input string may or may not contain the unit, so you will need to be able to handle
    /// that.
    #[allow(unused)]
    string_to_value: Option<Arc<dyn Fn(&String) -> Option<i32> + Send + Sync>>,
}

impl Display for StringParam {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value.as_ref().lock().unwrap().clone())
    }
}

impl Debug for StringParam {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // This uses the above `Display` instance to show the value
        write!(f, "{}: {}", &self.name, &self)
    }
}

// `Params` can not be implemented outside of NIH-plug itself because `ParamPtr` is also closed
impl super::Sealed for StringParam {}

impl Param for StringParam {
    type Plain = String;

    fn name(&self) -> &str {
        &self.name
    }

    fn unit(&self) -> &'static str {
        self.unit
    }

    fn poly_modulation_id(&self) -> Option<u32> {
        None
    }

    #[inline]
    fn modulated_plain_value(&self) -> Self::Plain {
        self.value.as_ref().lock().unwrap().clone()
    }

    #[inline]
    fn modulated_normalized_value(&self) -> f32 {
        0.0
    }

    #[inline]
    fn unmodulated_plain_value(&self) -> Self::Plain {
        self.value.as_ref().lock().unwrap().clone()
    }

    #[inline]
    fn unmodulated_normalized_value(&self) -> f32 {
        0.0
    }

    #[inline]
    fn default_plain_value(&self) -> Self::Plain {
        self.default.clone()
    }

    fn step_count(&self) -> Option<usize> {
        None
    }

    fn previous_step(&self, _from: Self::Plain, _finer: bool) -> Self::Plain {
        self.value.as_ref().lock().unwrap().clone()
    }

    fn next_step(&self, _from: Self::Plain, _finer: bool) -> Self::Plain {
        self.value.as_ref().lock().unwrap().clone()
    }

    fn normalized_value_to_string(&self, normalized: f32, include_unit: bool) -> String {
        let value = self.preview_plain(normalized);
        match (&self.value_to_string, include_unit) {
            (Some(f), true) => format!("{}{}", f(value), self.unit),
            (Some(f), false) => f(value),
            (None, true) => format!("{}{}", value, self.unit),
            (None, false) => format!("{value}"),
        }
    }

    fn string_to_normalized_value(&self, _string: &str) -> Option<f32> {
        None
    }

    #[inline]
    fn preview_normalized(&self, _plain: Self::Plain) -> f32 {
        0.0
    }

    #[inline]
    fn preview_plain(&self, _normalized: f32) -> Self::Plain {
        self.value.as_ref().lock().unwrap().clone()
    }

    fn flags(&self) -> ParamFlags {
        self.flags
    }

    fn as_ptr(&self) -> ParamPtr {
        ParamPtr::StringParam(self as *const _ as *mut _)
    }
}

impl ParamMut for StringParam {
    fn set_plain_value(&self, plain: Self::Plain) -> bool {
        if self.value() != plain {
            let mut h = self.value.lock().unwrap();
            *h = plain;

            // self.value = plain.clone(); // WHAT

            if let Some(f) = &self.value_changed {
                f(self.value.as_ref().lock().unwrap().clone());
            }
            true
        } else {
            false
        }

        // REAPER spams automation events with the same value. This prevents callbacks from firing
        // multiple times. This can be problematic when they're used to trigger expensive
        // computations when a parameter changes.
    }

    fn set_normalized_value(&self, normalized: f32) -> bool {
        // NOTE: The double conversion here is to make sure the state is reproducible. State is
        //       saved and restored using plain values, and the new normalized value will be
        //       different from `normalized`. This is not necessary for the modulation as these
        //       values are never shown to the host.
        self.set_plain_value(self.preview_plain(normalized))
    }

    fn modulate_value(&self, _modulation_offset: f32) -> bool {
        false
    }

    fn update_smoother(&self, _sample_rate: f32, _reset: bool) {
        // noop
    }
}

impl StringParam {
    /// Build a new [`StringParam`]. Use the other associated functions to modify the behavior of the
    /// parameter.
    pub fn new(name: impl Into<String>, default: String) -> Self {
        Self {
            default: default.clone(),
            value: Arc::new(Mutex::new(default)),

            flags: ParamFlags::default()
                .union(ParamFlags::HIDDEN)
                .union(ParamFlags::HIDE_IN_GENERIC_UI)
                .union(ParamFlags::NON_AUTOMATABLE),
            value_changed: None,

            name: name.into(),
            unit: "",
            value_to_string: None,
            string_to_value: None,
        }
    }

    pub fn set_value(&self, plain: String) -> bool {
        if self.value() != plain {
            let mut h = self.value.lock().unwrap();
            *h = plain;

            // self.value = plain.clone(); // WHAT

            if let Some(f) = &self.value_changed {
                f(self.value.as_ref().lock().unwrap().clone());
            }
            true
        } else {
            false
        }

        // REAPER spams automation events with the same value. This prevents callbacks from firing
        // multiple times. This can be problematic when they're used to trigger expensive
        // computations when a parameter changes.
    }

    /// The field's current plain value, after monophonic modulation has been applied. Equivalent to
    /// calling `param.plain_value()`.
    #[inline]
    pub fn value(&self) -> String {
        // changed return type
        self.modulated_plain_value() // kept
    }

    // /// Enable polyphonic modulation for this parameter. The ID is used to uniquely identify this
    // /// parameter in [`NoteEvent::PolyModulation`][crate::prelude::NoteEvent::PolyModulation]
    // /// events, and must thus be unique between _all_ polyphonically modulatable parameters. See the
    // /// event's documentation on how to use polyphonic modulation. Also consider configuring the
    // /// [`ClapPlugin::CLAP_POLY_MODULATION_CONFIG`][crate::prelude::ClapPlugin::CLAP_POLY_MODULATION_CONFIG]
    // /// constant when enabling this.
    // ///
    // /// # Important
    // ///
    // /// After enabling polyphonic modulation, the plugin **must** start sending
    // /// [`NoteEvent::VoiceTerminated`][crate::prelude::NoteEvent::VoiceTerminated] events to the
    // /// host when a voice has fully ended. This allows the host to reuse its modulation resources.
    // pub fn with_poly_modulation_id(mut self, id: u32) -> Self {
    //     // self.poly_modulation_id = Some(id);
    //     self
    // }

    // /// Set up a smoother that can gradually interpolate changes made to this parameter, preventing
    // /// clicks and zipper noises.
    // pub fn with_smoother(mut self, style: SmoothingStyle) -> Self {
    //     // // Logarithmic smoothing will cause problems if the range goes through zero since then you
    //     // // end up multiplying by zero
    //     // let goes_through_zero = match (&style, &self.range) {
    //     //     (SmoothingStyle::Logarithmic(_), IntRange::Linear { min, max }) => {
    //     //         *min == 0 || *max == 0 || min.signum() != max.signum()
    //     //     }
    //     //     _ => false,
    //     // };
    //     // nih_debug_assert!(
    //     //     !goes_through_zero,
    //     //     "Logarithmic smoothing does not work with ranges that go through zero"
    //     // );

    //     // self.smoothed = Smoother::new(style);
    //     self
    // }

    /// Run a callback whenever this parameter's value changes. The argument passed to this function
    /// is the parameter's new value. This should not do anything expensive as it may be called
    /// multiple times in rapid succession, and it can be run from both the GUI and the audio
    /// thread.
    pub fn with_callback(mut self, callback: Arc<dyn Fn(String) + Send + Sync>) -> Self {
        self.value_changed = Some(callback);
        self
    }

    /// Display a unit when rendering this parameter to a string. Appended after the
    /// [`value_to_string`][Self::with_value_to_string()] function if that is also set. NIH-plug
    /// will not automatically add a space before the unit.
    pub fn with_unit(mut self, unit: &'static str) -> Self {
        self.unit = unit;
        self
    }

    /// Use a custom conversion function to convert the plain, unnormalized value to a
    /// string.
    pub fn with_value_to_string(
        mut self,
        callback: Arc<dyn Fn(String) -> String + Send + Sync>,
    ) -> Self {
        self.value_to_string = Some(callback);
        self
    }

    // `with_step_size` is only implemented for the f32 version

    // /// Use a custom conversion function to convert from a string to a plain, unnormalized
    // /// value. If the string cannot be parsed, then this should return a `None`. If this
    // /// happens while the parameter is being updated then the update will be canceled.
    // ///
    // /// The input string may or may not contain the unit, so you will need to be able to handle
    // /// that.
    // pub fn with_string_to_value(
    //     mut self,
    //     callback: Arc<dyn Fn(&str) -> Option<String> + Send + Sync>,
    // ) -> Self {
    //     self.string_to_value = Some(callback);
    //     self
    // }

    /// Mark the parameter as non-automatable. This means that the parameter cannot be changed from
    /// an automation lane. The parameter can however still be manually changed by the user from
    /// either the plugin's own GUI or from the host's generic UI.
    pub fn non_automatable(mut self) -> Self {
        self.flags.insert(ParamFlags::NON_AUTOMATABLE);
        self
    }

    /// Hide the parameter in the host's generic UI for this plugin. This also implies
    /// `NON_AUTOMATABLE`. Setting this does not prevent you from changing the parameter in the
    /// plugin's editor GUI.
    pub fn hide(mut self) -> Self {
        self.flags.insert(ParamFlags::HIDDEN);
        self
    }

    /// Don't show this parameter when generating a generic UI for the plugin using one of
    /// NIH-plug's generic UI widgets.
    pub fn hide_in_generic_ui(mut self) -> Self {
        self.flags.insert(ParamFlags::HIDE_IN_GENERIC_UI);
        self
    }
}
