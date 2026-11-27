#![forbid(unsafe_code)]
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

//! Probe-only macOS 27+ Dock-swipe candidate: serialized session-tap gesture
//! with the IOHID payload in CGEvent data field 4205.
//!
//! This module is the internal, explicitly-opted candidate entry for a
//! guarded probe. It is deliberately NOT wired into any product dispatch
//! route: [`crate::space_switch_edges::gesture_payload_allowed`] still
//! refuses macOS >= 27 for the product path, pending the signed proof
//! described in `docs/changes/macos-0.1-native-spaces-prior-art.md`
//! ("Follow-through: Dock swipe and Mission Control"). Nothing here mutates
//! a preference; the native gesture setting is only read.
//!
//! Prior art (tagged local checkouts; line numbers in the tagged files):
//! - WhichSpace v1.5.0 (`07deeea8`) `WhichSpace/SpaceSwitcher.swift`:
//!   three-phase HID swipe (`postSwipeGesture` :936-961), Natural-scrolling
//!   direction inversion (:963-984), augmented event fields
//!   (`augmentedDockSwipe` :1099-1120), field-4205 payload append
//!   (`withIOHIDPayload` :1122-1140), IOHID queue/gesture/velocity layout
//!   and Q16.16 (`iohidPayload` :1142-1190, `fixedPoint` :1192-1205), and
//!   the macOS 27 gesture-setting precondition (:713-753).
//! - rift v0.8.6 (`f13c2513`) `src/sys/space_switch.rs`: version branch
//!   (:170-176), 27+ event fields (:506-548), BE TLV deserialize/serialize
//!   with field 4205 (:306-401), IOHID layouts and Q16.16 (:460-548).
//! - yabai v7.1.25 (`02de172f`) `src/space_manager.c:920-1002`: pre-27
//!   began/ended pair, unchanged and still owned by
//!   [`crate::space_switch_edges`].
//!
//! Shape that MUST hold before anything posts:
//! 1. macOS major is exactly 27 (unknown or other versions refuse);
//! 2. a horizontal three-/four-finger Space swipe is enabled for a tracked
//!    trackpad (read-only; this port never enables it privately);
//! 3. no mouse button is held (WindowServer drag tracking swallows synthetic
//!    swipes silently, so the caller must take the classic route);
//! 4. EVERY event of the plan (three phases per step, all steps) is built
//!    and serialized successfully, and the pre-post gates pass.
//!
//! Only then does [`PreparedGesture27::post_after`] begin posting. A partially
//! built plan refuses with zero posts, so there is no begun-without-ended
//! edge from construction failure.
//!
//! One-dispatch fence: a [`PreparedGesture27`] is consumed by `post_after` and
//! cannot be replayed. The candidate never retries and never synthesizes an
//! inverse.
//!
//! Pre-27 behavior is untouched: [`crate::space_switch_edges::gesture_event`]
//! and `post_gesture_pairs` keep the pinned yabai 6-field branch.
//!
//! This candidate never warps the pointer, opens Mission Control, or selects
//! a fullscreen Space. The fixture may set the event location for E1, as in
//! WhichSpace v1.5.0 `postSwipeGesture` (:955-957); this does not enable a
//! product route. The caller owns the focused-window and arrival proofs.

use crate::space_switch_edges::{
    GESTURE_FIELD_HID_TYPE, GESTURE_FIELD_MOTION, GESTURE_FIELD_PHASE, GESTURE_FIELD_PROGRESS,
    GESTURE_FIELD_TYPE, GESTURE_FIELD_VELOCITY_X, GESTURE_HID_DOCK_SWIPE,
    GESTURE_MOTION_HORIZONTAL, GESTURE_TYPE_DOCK_CONTROL,
};

/// The only macOS major this candidate is defined for (WhichSpace v1.5.0
/// gates its IOHID payload on `majorVersion >= 27`; this probe-only port
/// accepts exactly 27 because 28+ would need its own observed family
/// select, never an inferred extension).
pub const GESTURE_27_MAJOR: u32 = 27;

/// CG event field carrying the serialized IOHID queue element
/// (WhichSpace v1.5.0 `Field.iohidPayload`; rift v0.8.6
/// `K_CGEVENT_DATA_HID_FIELD`).
pub const GESTURE_27_FIELD_HID_PAYLOAD: u16 = 4205;
/// Serialized CGEvent data version accepted before appending field 4205
/// (WhichSpace v1.5.0 guards `starts(with: [0, 0, 0, 2])`).
pub const GESTURE_27_DATA_VERSION: i32 = 2;

/// WhichSpace v1.5.0 extra fields absent from the pre-27 yabai branch.
pub const GESTURE_27_FIELD_VELOCITY_Y: u32 = 130;
pub const GESTURE_27_FIELD_PHASE_MIRROR: u32 = 134;
pub const GESTURE_27_FIELD_FLAVOR: u32 = 138;
pub const GESTURE_27_FIELD_TIMESTAMP: u32 = 169;
pub const GESTURE_27_FIELD_POSITION_X: u32 = 125;
pub const GESTURE_27_FIELD_POSITION_Y: u32 = 126;

/// `kCGGesturePhaseChanged`; WhichSpace v1.5.0 requires all three phases
/// ("with only two, switching does not work while Mission Control is open").
pub const GESTURE_27_PHASE_CHANGED: i64 = 2;
/// The exact serialized-phase order WhichSpace v1.5.0 posts per step.
pub const GESTURE_27_PHASES: [i64; 3] = [
    crate::space_switch_edges::GESTURE_PHASE_BEGAN,
    GESTURE_27_PHASE_CHANGED,
    crate::space_switch_edges::GESTURE_PHASE_ENDED,
];

/// WhichSpace v1.5.0 base swipe velocity, scaled by step count.
pub const GESTURE_27_VELOCITY: f64 = 2000.0;
/// WhichSpace v1.5.0 constant flavor (`IOHID.flavorDockPrimary`).
pub const GESTURE_27_FLAVOR_DOCK_PRIMARY: f64 = 3.0;
/// WhichSpace v1.5.0 constant swipe position X.
pub const GESTURE_27_POSITION_X: f64 = 0.1;

/// IOHID queue/gesture/velocity constants (WhichSpace v1.5.0 `IOHID`,
/// rift v0.8.6 `K_IOHID_EVENT_TYPE_*`).
pub const IOHID_EVENT_TYPE_VELOCITY: u32 = 9;
pub const IOHID_EVENT_TYPE_FLUID_TOUCH_GESTURE: u32 = 23;
pub const IOHID_GESTURE_FLAVOR_DOCK_PRIMARY: u16 = 3;
/// Fixed `MemoryLayout` sizes in WhichSpace v1.5.0 (gesture 40, velocity 28,
/// queue header 28; rift's `hid_layout_sizes_match_expected` asserts the
/// same 28/40/28).
pub const IOHID_SYSTEM_QUEUE_ELEMENT_SIZE: usize = 28;
pub const IOHID_FLUID_TOUCH_GESTURE_SIZE: usize = 40;
pub const IOHID_VELOCITY_EVENT_SIZE: usize = 28;
pub const IOHID_EVENT_COUNT: u32 = 2;
pub const IOHID_PHASE_SHIFT: u32 = 24;

/// Q16.16 scale (`fixedPoint` in WhichSpace v1.5.0).
pub const FIXED_16_16_SCALE: f64 = 65_536.0;

/// One CGEvent field write from the tagged 27+ shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum SyntheticValue {
    Integer(i64),
    Double(f64),
}

/// The exact WhichSpace v1.5.0 `augmentedDockSwipe` field writes, in source
/// order, minus the dynamic timestamp (field 169) which the live builder
/// stamps per event. `goes_right` is already the post-Natural-scrolling
/// direction.
#[must_use]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn gesture_27_fields(
    phase: i64,
    goes_right: bool,
    velocity: f64,
) -> [(u32, SyntheticValue); 10] {
    let progress = if goes_right { 1.0 } else { -1.0 };
    let signed_velocity = if goes_right { velocity } else { -velocity };
    [
        (
            GESTURE_FIELD_TYPE,
            SyntheticValue::Integer(GESTURE_TYPE_DOCK_CONTROL),
        ),
        (
            GESTURE_FIELD_HID_TYPE,
            SyntheticValue::Integer(GESTURE_HID_DOCK_SWIPE),
        ),
        (GESTURE_FIELD_PHASE, SyntheticValue::Integer(phase)),
        (GESTURE_FIELD_PROGRESS, SyntheticValue::Double(progress)),
        (
            GESTURE_FIELD_MOTION,
            SyntheticValue::Integer(GESTURE_MOTION_HORIZONTAL),
        ),
        (
            GESTURE_FIELD_VELOCITY_X,
            SyntheticValue::Double(signed_velocity),
        ),
        (GESTURE_27_FIELD_VELOCITY_Y, SyntheticValue::Double(0.0)),
        (
            GESTURE_27_FIELD_PHASE_MIRROR,
            SyntheticValue::Integer(phase),
        ),
        (
            GESTURE_27_FIELD_FLAVOR,
            SyntheticValue::Double(GESTURE_27_FLAVOR_DOCK_PRIMARY),
        ),
        (
            GESTURE_27_FIELD_POSITION_X,
            SyntheticValue::Double(GESTURE_27_POSITION_X),
        ),
    ]
}

/// WhichSpace v1.5.0 `isNaturalScrollingEnabled`: the key is absent until
/// the user changes the setting and the system default is on.
#[must_use]
pub const fn natural_scrolling_from_preference(value: Option<i64>) -> bool {
    match value {
        Some(v) => v != 0,
        None => true,
    }
}

/// WhichSpace v1.5.0 `augmentedSwipeGoesRight`: Dock reverses HID-backed
/// swipes when Natural scrolling is off, so the requested direction flips.
#[must_use]
pub const fn augmented_swipe_goes_right(requested_right: bool, natural_scrolling: bool) -> bool {
    if natural_scrolling {
        requested_right
    } else {
        !requested_right
    }
}

/// WhichSpace v1.5.0 `hasEnabledSpaceSwipeGesture`: any tracked trackpad
/// three-/four-finger horizontal Space-swipe preference that is non-zero.
/// The six values are AppleMultitouch 3/4, Bluetooth trackpad 3/4, then the
/// global 3/4 keys.
#[must_use]
pub fn has_enabled_space_swipe_gesture(values: &[Option<i64>]) -> bool {
    values.iter().any(|value| value.unwrap_or(0) != 0)
}

/// Whether the candidate is defined for this macOS major. Exactly 27 is
/// accepted; unknown and every other major refuse (never an inferred family
/// select).
pub fn candidate_27_allowed(major: Option<u32>) -> Result<(), String> {
    match major {
        Some(GESTURE_27_MAJOR) => Ok(()),
        Some(m) => Err(format!(
            "gesture 27 candidate refused on macOS major {m}: the serialized candidate is defined for exactly major 27; other releases need their own observed family select"
        )),
        None => Err("gesture 27 candidate refused: macOS major unreadable".to_owned()),
    }
}

/// WhichSpace v1.5.0 `fixedPoint`: Q16.16 with saturation and the non-zero
/// epsilon clamp (a nonzero value must never truncate to zero). Byte-for-byte
/// golden values live in the tests.
#[must_use]
pub fn fixed_16_16(value: f64) -> i32 {
    let scaled = value * FIXED_16_16_SCALE;
    if scaled >= f64::from(i32::MAX) {
        return i32::MAX;
    }
    if scaled <= f64::from(i32::MIN) {
        return i32::MIN;
    }
    let fixed = scaled as i32; // Rust `as` saturates; value is finite here.
    if fixed == 0 && value != 0.0 {
        return if value > 0.0 { 1 } else { -1 };
    }
    fixed
}

/// Plain-data preconditions for the 27+ candidate. The live reader is
/// [`live_preconditions`]; tests inject literals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Gesture27Preconditions {
    pub(crate) macos_major: Option<u32>,
    pub(crate) natural_scrolling: bool,
    pub(crate) space_swipe_gesture_enabled: bool,
    pub(crate) mouse_button_pressed: bool,
}

/// Fresh read-only preconditions: macOS major, Natural scrolling, the
/// horizontal Space-swipe setting, and the held-button state. No writes.
#[must_use]
pub(crate) fn live_preconditions() -> Gesture27Preconditions {
    #[cfg(target_os = "macos")]
    {
        Gesture27Preconditions {
            macos_major: crate::ffi::macos_major(),
            natural_scrolling: natural_scrolling_from_preference(
                crate::ffi::natural_scrolling_preference(),
            ),
            space_swipe_gesture_enabled: has_enabled_space_swipe_gesture(
                &crate::ffi::space_swipe_gesture_preferences(),
            ),
            mouse_button_pressed: crate::ffi::mouse_buttons_pressed(),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Gesture27Preconditions {
            macos_major: None,
            natural_scrolling: true,
            space_swipe_gesture_enabled: false,
            mouse_button_pressed: false,
        }
    }
}

/// The serialized IOHID payload exactly as WhichSpace v1.5.0
/// `iohidPayload` builds it. WhichSpace appends the arm64-native structs with
/// `withUnsafeBytes`, so every interior scalar is little-endian; only the
/// enclosing CGEvent-data TLV header (`element_size`, `tag|field`) is
/// big-endian (see [`append_hid_payload_field`]). Inputs are the CGEvent
/// field values; the payload's progress and velocity are negated Q16.16
/// (WhichSpace writes `fixedPoint(-progress)` / `fixedPoint(-velocity)`), its
/// mask is 0, its motion is 1, its flavor is 3, and it always carries both a
/// fluid-touch gesture event and a velocity event (`eventCount: 2`).
#[must_use]
pub fn hid_payload_bytes(
    phase: i64,
    progress: f64,
    position_x: f64,
    position_y: f64,
    velocity_x: f64,
    timestamp: u64,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(
        IOHID_SYSTEM_QUEUE_ELEMENT_SIZE
            + IOHID_FLUID_TOUCH_GESTURE_SIZE
            + IOHID_VELOCITY_EVENT_SIZE,
    );
    // IOHIDSystemQueueElementHeader (native LE).
    out.extend_from_slice(&timestamp.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes()); // senderID
    out.extend_from_slice(&0u32.to_le_bytes()); // options
    out.extend_from_slice(&0u32.to_le_bytes()); // attributeLength
    out.extend_from_slice(&IOHID_EVENT_COUNT.to_le_bytes());
    // IOHIDFluidTouchGestureData (native LE).
    out.extend_from_slice(&(IOHID_FLUID_TOUCH_GESTURE_SIZE as u32).to_le_bytes());
    out.extend_from_slice(&IOHID_EVENT_TYPE_FLUID_TOUCH_GESTURE.to_le_bytes());
    out.extend_from_slice(&((phase as u32 & 0xFF) << IOHID_PHASE_SHIFT).to_le_bytes());
    out.push(0); // depth
    out.extend_from_slice(&[0; 3]); // reserved
    out.extend_from_slice(&fixed_16_16(position_x).to_le_bytes());
    out.extend_from_slice(&fixed_16_16(position_y).to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes()); // positionZ
    out.extend_from_slice(&0u32.to_le_bytes()); // swipeMask
    out.extend_from_slice(&(GESTURE_MOTION_HORIZONTAL as u16).to_le_bytes());
    out.extend_from_slice(&IOHID_GESTURE_FLAVOR_DOCK_PRIMARY.to_le_bytes());
    out.extend_from_slice(&fixed_16_16(-progress).to_le_bytes());
    // IOHIDVelocityEventData (native LE).
    out.extend_from_slice(&(IOHID_VELOCITY_EVENT_SIZE as u32).to_le_bytes());
    out.extend_from_slice(&IOHID_EVENT_TYPE_VELOCITY.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // options
    out.push(1); // depth
    out.extend_from_slice(&[0; 3]); // reserved
    out.extend_from_slice(&fixed_16_16(-velocity_x).to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes()); // velocityY
    out.extend_from_slice(&0i32.to_le_bytes()); // velocityZ
    out
}

/// Append the BE TLV field 4205 entry to freshly serialized CGEvent data,
/// exactly as WhichSpace v1.5.0 `withIOHIDPayload` does: validate the version
/// 2 prefix, then append the BE `element_size` / `tag|field` header and the
/// payload bytes. The existing serialized bytes are copied through unchanged
/// (one output allocation); this deliberately does not parse or re-encode
/// unrelated fields.
pub fn append_hid_payload_field(serialized: &[u8], payload: &[u8]) -> Result<Vec<u8>, String> {
    let payload_size = u16::try_from(payload.len())
        .map_err(|_| format!("payload {} exceeds the u16 TLV size", payload.len()))?;
    let version = serialized
        .get(..4)
        .ok_or_else(|| "serialized CGEvent data is shorter than its version".to_owned())?;
    let version = i32::from_be_bytes(version.try_into().expect("four bytes"));
    if version != GESTURE_27_DATA_VERSION {
        return Err(format!(
            "serialized CGEvent data version {version} is not {GESTURE_27_DATA_VERSION}; refusing to append field {GESTURE_27_FIELD_HID_PAYLOAD}"
        ));
    }

    let mut out = Vec::with_capacity(serialized.len() + 4 + payload.len());
    out.extend_from_slice(serialized);
    out.extend_from_slice(&payload_size.to_be_bytes());
    // Field 4205 = 0x106D leaves the tag bits (top two) zero, exactly what
    // WhichSpace's byte append produces.
    out.extend_from_slice(&GESTURE_27_FIELD_HID_PAYLOAD.to_be_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

/// A fully prepared, single-use 27+ dispatch. Holding one proves every event
/// was built and serialized before any post (the "no beginning until
/// preparations pass" fence).
#[cfg(target_os = "macos")]
pub(crate) struct PreparedGesture27 {
    events: Vec<objc2_core_foundation::CFRetained<objc2_core_graphics::CGEvent>>,
    requested_right: bool,
}

/// Non-macOS placeholder so the public API shape is identical everywhere.
#[cfg(not(target_os = "macos"))]
pub(crate) struct PreparedGesture27 {
    _mac_only: (),
}

#[cfg(target_os = "macos")]
#[allow(dead_code)] // Guarded-probe API; no product route may call it yet.
impl PreparedGesture27 {
    /// Set the reconstructed event location without moving the pointer.
    pub(crate) fn locate(&self, location: objc2_core_foundation::CGPoint) {
        for event in &self.events {
            objc2_core_graphics::CGEvent::set_location(Some(event), location);
        }
    }

    /// Number of prepared events (three phases per step).
    #[must_use]
    pub(crate) fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Effective direction after the Natural-scrolling inversion.
    #[must_use]
    pub(crate) fn requested_right(&self) -> bool {
        self.requested_right
    }

    /// Post the prepared plan in order: all three phases of step 1, then step
    /// 2, and so on (WhichSpace v1.5.0 `postSwipeGesture` per step).
    ///
    /// Before the first post this rechecks the shared synthetic gates and the
    /// held-button precondition, then runs `boundary` (the probe's exact
    /// held-helper/oracle check). Any failure posts nothing. The plan is
    /// consumed: one dispatch, no replay.
    pub(crate) fn post_after(
        self,
        boundary: impl FnOnce() -> Result<(), String>,
    ) -> Result<u32, String> {
        use objc2_core_graphics::{CGEvent, CGEventTapLocation};
        crate::space_switch_edges::edge_synth_gates("gesture-27-post")
            .map_err(|e| e.to_string())?;
        if crate::ffi::mouse_buttons_pressed() {
            return Err(
                "gesture 27 candidate refused pre-post: a mouse button is pressed; WindowServer drag tracking swallows synthetic swipes (classic route required)"
                    .to_owned(),
            );
        }
        boundary()?;
        let mut posted = 0u32;
        for event in &self.events {
            CGEvent::post(CGEventTapLocation::SessionEventTap, Some(event));
            posted += 1;
        }
        Ok(posted)
    }
}

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)] // Guarded-probe API stub; no product route may call it.
impl PreparedGesture27 {
    pub(crate) fn post_after(
        self,
        _boundary: impl FnOnce() -> Result<(), String>,
    ) -> Result<u32, String> {
        Err("gesture 27 candidate posts are macOS-only".to_owned())
    }
}

/// Validate `pre` and build every event of the plan, or refuse with zero
/// posts. `sign` follows [`crate::space_switch_edges::swipe_plan`] /
/// `gesture_sign`: positive means the target sits at a higher native index.
/// `pairs` is the absolute index distance.
///
/// Probe-only entry: not reachable from product dispatch. Consumers must be
/// explicit that they are testing the 27+ candidate, and the product gate in
/// [`crate::space_switch_edges::gesture_payload_allowed`] still refuses 27+.
#[allow(dead_code)] // Guarded-probe entry point; no product route may call it yet.
pub(crate) fn prepare_with(
    pre: &Gesture27Preconditions,
    sign: f64,
    pairs: usize,
) -> Result<PreparedGesture27, String> {
    candidate_27_allowed(pre.macos_major)?;
    if !pre.space_swipe_gesture_enabled {
        return Err(
            "gesture 27 candidate refused: no horizontal three-/four-finger Space swipe gesture is enabled for a tracked trackpad; enable one in System Settings (read-only check, no settings write)"
                .to_owned(),
        );
    }
    if pre.mouse_button_pressed {
        return Err(
            "gesture 27 candidate refused: a mouse button is pressed; WindowServer drag tracking swallows synthetic swipes (classic route required)"
                .to_owned(),
        );
    }
    if !sign.is_finite() || sign == 0.0 {
        return Err(format!(
            "gesture 27 candidate refused: direction sign {sign} is not finite and nonzero"
        ));
    }
    if pairs == 0 {
        return Err(
            "gesture 27 candidate refused: a plan of zero steps has no direction".to_owned(),
        );
    }
    let requested_right = sign > 0.0;
    let goes_right = augmented_swipe_goes_right(requested_right, pre.natural_scrolling);
    // WhichSpace v1.5.0 scales the base velocity by step count so multi-Space
    // jumps stay instant.
    #[allow(clippy::cast_precision_loss)]
    let velocity = GESTURE_27_VELOCITY * pairs as f64;

    #[cfg(target_os = "macos")]
    {
        let mut events = Vec::with_capacity(pairs * GESTURE_27_PHASES.len());
        for _ in 0..pairs {
            for phase in GESTURE_27_PHASES {
                events.push(build_gesture_27_event(phase, goes_right, velocity)?);
            }
        }
        Ok(PreparedGesture27 {
            events,
            requested_right,
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (goes_right, velocity, requested_right);
        Err("gesture 27 candidate events are macOS-only".to_owned())
    }
}

/// Live guarded-probe preparation: shared synthetic gates first, then fresh
/// read-only preconditions, then the full build. Any failure posts nothing.
#[allow(dead_code)] // Guarded-probe entry point; product dispatch must not call it.
pub(crate) fn prepare_gesture_27(sign: f64, pairs: usize) -> Result<PreparedGesture27, String> {
    crate::space_switch_edges::edge_synth_gates("gesture-27-prepare").map_err(|e| e.to_string())?;
    prepare_with(&live_preconditions(), sign, pairs)
}

/// Build one 27+ event: the tagged field shape, a fresh mach timestamp, the
/// 4205 IOHID payload read back from the event's own fields, and the
/// own-switch provenance tag.
#[cfg(target_os = "macos")]
fn build_gesture_27_event(
    phase: i64,
    goes_right: bool,
    velocity: f64,
) -> Result<objc2_core_foundation::CFRetained<objc2_core_graphics::CGEvent>, String> {
    use objc2_core_foundation::CFData;
    use objc2_core_graphics::{CGEvent, CGEventField};

    let bytes = serialized_gesture_27_event(phase, goes_right, velocity)?;
    let data = CFData::from_bytes(&bytes);
    let event = CGEvent::from_data(None, Some(&data)).ok_or_else(|| {
        "gesture 27 candidate refused: event reconstruction returned null".to_owned()
    })?;
    // `CGEventCreateData` does not serialize `EventSourceUserData` (the field
    // lives on the event source, and a plain CGEventCreate/CreateData/
    // CreateFromData round-trip reads back 0), so setting it before
    // serialization would be lost. Apply it on the reconstructed event
    // instead, mirroring [`crate::space_switch_edges::gesture_event`]; this
    // write cannot disturb field 4205, which was already accepted by
    // `CGEventCreateFromData` from the appended TLV.
    CGEvent::set_integer_value_field(
        Some(&event),
        CGEventField::EventSourceUserData,
        crate::input_match::OWN_SWITCH_USER_DATA,
    );
    Ok(event)
}

/// The pre-reconstruction serialized event data (kept separate so tests can
/// inspect the exact bytes without assuming a CGEventCreateData round-trip
/// preserves the appended blob).
///
/// The own-switch provenance tag is deliberately absent here: it is applied
/// to the reconstructed event in [`build_gesture_27_event`], because
/// `CGEventCreateData` drops `EventSourceUserData`.
#[cfg(target_os = "macos")]
fn serialized_gesture_27_event(
    phase: i64,
    goes_right: bool,
    velocity: f64,
) -> Result<Vec<u8>, String> {
    use objc2_core_graphics::{CGEvent, CGEventField};

    let event = CGEvent::new(None)
        .ok_or_else(|| "gesture 27 candidate refused: event creation returned null".to_owned())?;
    for (field, value) in gesture_27_fields(phase, goes_right, velocity) {
        match value {
            SyntheticValue::Integer(v) => {
                CGEvent::set_integer_value_field(Some(&event), CGEventField(field), v);
            }
            SyntheticValue::Double(v) => {
                CGEvent::set_double_value_field(Some(&event), CGEventField(field), v);
            }
        }
    }
    let stamped = crate::ffi::mach_absolute_time_ticks();
    CGEvent::set_double_value_field(
        Some(&event),
        CGEventField(GESTURE_27_FIELD_TIMESTAMP),
        stamped as f64,
    );

    let timestamp = {
        let event_timestamp = CGEvent::timestamp(Some(&event));
        if event_timestamp == 0 {
            stamped
        } else {
            event_timestamp
        }
    };
    let progress = CGEvent::double_value_field(Some(&event), CGEventField(GESTURE_FIELD_PROGRESS));
    let position_x =
        CGEvent::double_value_field(Some(&event), CGEventField(GESTURE_27_FIELD_POSITION_X));
    let position_y =
        CGEvent::double_value_field(Some(&event), CGEventField(GESTURE_27_FIELD_POSITION_Y));
    let velocity_x =
        CGEvent::double_value_field(Some(&event), CGEventField(GESTURE_FIELD_VELOCITY_X));
    let payload = hid_payload_bytes(
        phase, progress, position_x, position_y, velocity_x, timestamp,
    );
    let serialized = CGEvent::new_data(None, Some(&event)).ok_or_else(|| {
        "gesture 27 candidate refused: event serialization returned null".to_owned()
    })?;
    append_hid_payload_field(&serialized.to_vec(), &payload)
        .map_err(|e| format!("gesture 27 candidate refused: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live_pre() -> Gesture27Preconditions {
        Gesture27Preconditions {
            macos_major: Some(GESTURE_27_MAJOR),
            natural_scrolling: true,
            space_swipe_gesture_enabled: true,
            mouse_button_pressed: false,
        }
    }

    fn le_i32(bytes: &[u8], offset: usize) -> i32 {
        i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn le_u16(bytes: &[u8], offset: usize) -> u16 {
        u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
    }

    fn le_u32(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn le_u64(bytes: &[u8], offset: usize) -> u64 {
        u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
    }

    // ---- Version / settings / button refusal ----

    #[test]
    fn candidate_gate_accepts_only_exactly_major_27() {
        assert!(candidate_27_allowed(Some(27)).is_ok());
        assert!(candidate_27_allowed(Some(26)).is_err());
        assert!(candidate_27_allowed(Some(28)).is_err());
        assert!(candidate_27_allowed(None).is_err());
    }

    #[test]
    fn natural_scrolling_defaults_on_and_is_literal() {
        // WhichSpace v1.5.0 `isNaturalScrollingEnabled`: absent -> on.
        assert!(natural_scrolling_from_preference(None));
        assert!(natural_scrolling_from_preference(Some(1)));
        assert!(natural_scrolling_from_preference(Some(-1)));
        assert!(!natural_scrolling_from_preference(Some(0)));
    }

    #[test]
    fn native_space_swipe_setting_requires_one_nonzero_value() {
        // WhichSpace v1.5.0 `hasEnabledSpaceSwipeGesture`: `(value ?? 0) != 0`.
        assert!(has_enabled_space_swipe_gesture(&[
            None,
            Some(0),
            Some(2),
            None,
            None,
            None
        ]));
        assert!(has_enabled_space_swipe_gesture(&[
            Some(1),
            None,
            None,
            None,
            None,
            None
        ]));
        assert!(!has_enabled_space_swipe_gesture(&[
            None,
            Some(0),
            Some(0),
            None,
            None,
            None
        ]));
        assert!(!has_enabled_space_swipe_gesture(&[]));
    }

    #[test]
    fn prepare_refuses_zero_posts_on_every_precondition() {
        let refused = |pre: Gesture27Preconditions| prepare_with(&pre, 1.0, 1).is_err();
        assert!(refused(Gesture27Preconditions {
            macos_major: None,
            ..live_pre()
        }));
        assert!(refused(Gesture27Preconditions {
            macos_major: Some(28),
            ..live_pre()
        }));
        assert!(refused(Gesture27Preconditions {
            space_swipe_gesture_enabled: false,
            ..live_pre()
        }));
        assert!(refused(Gesture27Preconditions {
            mouse_button_pressed: true,
            ..live_pre()
        }));
        assert!(prepare_with(&live_pre(), 0.0, 1).is_err());
        assert!(prepare_with(&live_pre(), 1.0, 0).is_err());
        assert!(prepare_with(&live_pre(), f64::NAN, 1).is_err());
        assert!(prepare_with(&live_pre(), f64::INFINITY, 1).is_err());
    }

    // ---- Independent literal goldens: BE TLV field 4205 ----

    #[test]
    fn field_4205_header_is_literal_big_endian() {
        // 4205 = 0x106D (field), payload 96 = 0x0060 (element size), tag 0.
        let payload = vec![0xA5; 96];
        let input = [0, 0, 0, 2, 0, 1, 0, 55, 0, 0, 0, 0, 0, 0, 0, 30];
        let out = append_hid_payload_field(&input, &payload).unwrap();
        assert_eq!(&out[..4], &[0, 0, 0, 2]);
        // Existing field 55 (i64 = 30) preserved byte-for-byte.
        assert_eq!(&out[4..16], &input[4..16]);
        // Literal BE TLV header for field 4205, then the payload.
        assert_eq!(&out[16..20], &[0x00, 0x60, 0x10, 0x6D]);
        assert_eq!(&out[20..], &payload[..]);
        assert_eq!(out.len(), 16 + 4 + 96);
    }

    #[test]
    fn field_4205_is_appended_after_the_existing_bytes_unchanged() {
        // WhichSpace v1.5.0 appends to `event.data` without parsing or
        // re-ordering; every byte of the freshly serialized event survives.
        let input = [
            0, 0, 0, 2, // version
            0, 1, 0, 100, 0, 0, 0, 0, 0, 0, 0, 1, // field 100 i64=1
            0, 1, 0, 7, 0, 0, 0, 0, 0, 0, 0, 2, // field 7 i64=2
        ];
        let payload = [1u8, 2, 3, 4];
        let out = append_hid_payload_field(&input, &payload).unwrap();
        assert_eq!(&out[..input.len()], &input);
        assert_eq!(
            &out[input.len()..input.len() + 4],
            &[0x00, 0x04, 0x10, 0x6D]
        );
        assert_eq!(&out[input.len() + 4..], &payload[..]);
        assert_eq!(out.len(), input.len() + 4 + payload.len());
    }

    #[test]
    fn serialized_event_data_requires_version_2_on_append() {
        let payload = [0u8; 8];
        assert!(append_hid_payload_field(&[0, 0, 0, 1], &payload).is_err());
        assert!(append_hid_payload_field(&[0, 0, 0], &payload).is_err());
        assert!(append_hid_payload_field(&[], &payload).is_err());
        // A bare valid-version prefix is appendable: WhichSpace checks only
        // `starts(with: [0, 0, 0, 2])` before appending.
        let bare = append_hid_payload_field(&[0, 0, 0, 2], &payload).unwrap();
        assert_eq!(&bare[..4], &[0, 0, 0, 2]);
        assert_eq!(&bare[4..8], &[0x00, 0x08, 0x10, 0x6D]);
        assert_eq!(&bare[8..], &payload[..]);
    }

    // ---- Independent literal goldens: Q16.16 ----

    #[test]
    fn q16_16_literals_match_tagged_fixed_point() {
        // Literal scaled integers, not values recomputed through the helper.
        assert_eq!(fixed_16_16(0.0), 0x0000_0000);
        assert_eq!(fixed_16_16(1.0), 0x0001_0000);
        assert_eq!(fixed_16_16(-1.0), -65_536);
        assert_eq!(fixed_16_16(0.1), 0x0000_1999);
        assert_eq!(fixed_16_16(-0.1), -6_553);
        assert_eq!(fixed_16_16(2000.0), 0x07D0_0000);
        assert_eq!(fixed_16_16(-2000.0), -131_072_000);
        assert_eq!(fixed_16_16(4000.0), 0x0FA0_0000);
        // Nonzero epsilon never truncates to zero.
        assert_eq!(fixed_16_16(f64::MIN_POSITIVE), 1);
        assert_eq!(fixed_16_16(-f64::MIN_POSITIVE), -1);
        // Saturation at the Int32 bounds.
        assert_eq!(fixed_16_16(32_768.0), i32::MAX);
        assert_eq!(fixed_16_16(-32_768.0), i32::MIN);
        assert_eq!(fixed_16_16(1e12), i32::MAX);
        assert_eq!(fixed_16_16(-1e12), i32::MIN);
    }

    // ---- Independent literal golden: full 96-byte IOHID payload ----

    #[test]
    fn hid_payload_is_the_literal_96_byte_whichspace_shape() {
        // Began phase, CG progress +1.0, position X 0.1, velocity +2000,
        // literal timestamp. Every byte below is written independently from
        // the WhichSpace v1.5.0 tagged struct layout (28 + 40 + 28) appended
        // as arm64-native little-endian; only the outer TLV header is BE.
        let payload = hid_payload_bytes(1, 1.0, 0.1, 0.0, 2000.0, 0x0102_0304_0506_0708);
        #[rustfmt::skip]
        let expected: [u8; 96] = [
            // IOHIDSystemQueueElementHeader (28, native LE)
            0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, // timestamp
            0, 0, 0, 0, 0, 0, 0, 0,                         // senderID
            0, 0, 0, 0,                                     // options
            0, 0, 0, 0,                                     // attributeLength
            2, 0, 0, 0,                                     // eventCount
            // IOHIDFluidTouchGestureData.base (16, native LE)
            40, 0, 0, 0,                                    // size
            23, 0, 0, 0,                                    // type (fluid touch)
            0, 0, 0, 0x01,                                  // options = phase 1 << 24
            0,                                              // depth
            0, 0, 0,                                        // reserved
            // gesture values (24, native LE)
            0x99, 0x19, 0, 0,                               // positionX 0.1 Q16.16
            0, 0, 0, 0,                                     // positionY
            0, 0, 0, 0,                                     // positionZ
            0, 0, 0, 0,                                     // swipeMask
            1, 0,                                           // motion horizontal
            3, 0,                                           // flavor dock primary
            0, 0, 0xFF, 0xFF,                               // progress -1.0 Q16.16
            // IOHIDVelocityEventData (28, native LE)
            28, 0, 0, 0,                                    // size
            9, 0, 0, 0,                                     // type (velocity)
            0, 0, 0, 0,                                     // options
            1,                                              // depth
            0, 0, 0,                                        // reserved
            0, 0, 0x30, 0xF8,                               // velocityX -2000 Q16.16
            0, 0, 0, 0,                                     // velocityY
            0, 0, 0, 0,                                     // velocityZ
        ];
        assert_eq!(payload, expected);
        assert_eq!(payload.len(), 96);
        assert_eq!(
            payload.len(),
            IOHID_SYSTEM_QUEUE_ELEMENT_SIZE
                + IOHID_FLUID_TOUCH_GESTURE_SIZE
                + IOHID_VELOCITY_EVENT_SIZE
        );
        // Literal native windows.
        assert_eq!(le_u64(&payload, 0), 0x0102_0304_0506_0708);
        assert_eq!(le_u32(&payload, 24), 2);
        assert_eq!(le_u32(&payload, 56), 0); // swipeMask
        assert_eq!(le_u16(&payload, 60), 1);
        assert_eq!(le_u16(&payload, 62), 3);
        assert_eq!(le_i32(&payload, 44), 0x0000_1999);
        assert_eq!(le_i32(&payload, 64), -65_536);
        assert_eq!(le_i32(&payload, 84), -131_072_000);
    }

    // ---- Independent literal goldens: phase + direction inversion ----

    #[test]
    fn phase_options_are_literal_phase_shifted_bytes() {
        for (phase, literal) in [
            (1i64, [0x00u8, 0, 0, 0x01]),
            (2, [0x00, 0, 0, 0x02]),
            (4, [0x00, 0, 0, 0x04]),
        ] {
            let payload = hid_payload_bytes(phase, 1.0, 0.1, 0.0, 2000.0, 7);
            assert_eq!(&payload[36..40], &literal, "phase {phase} options");
        }
        assert_eq!(GESTURE_27_PHASES, [1, 2, 4]);
        assert_eq!(GESTURE_27_PHASE_CHANGED, 2);
    }

    #[test]
    fn natural_scrolling_inverts_only_the_hid_direction() {
        // WhichSpace v1.5.0 `augmentedSwipeGoesRight`.
        assert!(augmented_swipe_goes_right(true, true));
        assert!(!augmented_swipe_goes_right(false, true));
        assert!(!augmented_swipe_goes_right(true, false));
        assert!(augmented_swipe_goes_right(false, false));
    }

    #[test]
    fn cg_progress_is_plus_minus_one_and_payload_progress_is_inverted() {
        // Tagged 27+ branch uses normal +/-1.0 in CG field 124 (NOT the
        // legacy Float.leastNonzeroMagnitude, which belongs to the pre-HID
        // path only). The HID payload negates it.
        let right = gesture_27_fields(1, true, 2000.0);
        assert_eq!(
            right[3],
            (GESTURE_FIELD_PROGRESS, SyntheticValue::Double(1.0))
        );
        assert_eq!(
            right[5],
            (GESTURE_FIELD_VELOCITY_X, SyntheticValue::Double(2000.0))
        );
        let right_payload = hid_payload_bytes(1, 1.0, 0.1, 0.0, 2000.0, 0);
        assert_eq!(le_i32(&right_payload, 64), -65536);

        let left = gesture_27_fields(1, false, 2000.0);
        assert_eq!(
            left[3],
            (GESTURE_FIELD_PROGRESS, SyntheticValue::Double(-1.0))
        );
        assert_eq!(
            left[5],
            (GESTURE_FIELD_VELOCITY_X, SyntheticValue::Double(-2000.0))
        );
        let left_payload = hid_payload_bytes(1, -1.0, 0.1, 0.0, -2000.0, 0);
        assert_eq!(le_i32(&left_payload, 64), 65_536);
        assert_eq!(le_i32(&left_payload, 84), 131_072_000);
    }

    #[test]
    fn requested_right_with_natural_off_posts_the_left_shape() {
        let pre = Gesture27Preconditions {
            natural_scrolling: false,
            ..live_pre()
        };
        let goes_right = augmented_swipe_goes_right(true, pre.natural_scrolling);
        assert!(!goes_right);
        let fields = gesture_27_fields(4, goes_right, 2000.0);
        assert_eq!(fields[0], (55, SyntheticValue::Integer(30)));
        assert_eq!(fields[3], (124, SyntheticValue::Double(-1.0)));
        assert_eq!(fields[5], (129, SyntheticValue::Double(-2000.0)));
    }

    #[test]
    fn field_plan_matches_the_tagged_27_plus_shape() {
        let fields = gesture_27_fields(4, true, 4000.0);
        assert_eq!(
            fields,
            [
                (55, SyntheticValue::Integer(30)),
                (110, SyntheticValue::Integer(23)),
                (132, SyntheticValue::Integer(4)),
                (124, SyntheticValue::Double(1.0)),
                (123, SyntheticValue::Integer(1)),
                (129, SyntheticValue::Double(4000.0)),
                (130, SyntheticValue::Double(0.0)),
                (134, SyntheticValue::Integer(4)),
                (138, SyntheticValue::Double(3.0)),
                (125, SyntheticValue::Double(0.1)),
            ]
        );
    }

    // ---- Live, effect-free construction checks (macOS only) ----

    #[cfg(target_os = "macos")]
    #[test]
    fn prepared_plan_builds_three_phases_per_step_before_any_post() {
        use objc2_core_graphics::{CGEvent, CGEventField};
        let pre = live_pre();
        let one = prepare_with(&pre, 1.0, 1).expect("one-step preparation is effect-free");
        assert_eq!(one.event_count(), 3);
        assert!(one.requested_right());
        let phases: Vec<i64> = one
            .events
            .iter()
            .map(|event| {
                CGEvent::integer_value_field(Some(event), CGEventField(GESTURE_FIELD_PHASE))
            })
            .collect();
        assert_eq!(phases, vec![1, 2, 4]);

        let two = prepare_with(&pre, -1.0, 2).expect("two-step preparation is effect-free");
        assert_eq!(two.event_count(), 6);
        assert!(!two.requested_right());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn prepared_events_read_back_tagged_fields_payload_and_own_tag() {
        use objc2_core_graphics::{CGEvent, CGEventField};
        let event = build_gesture_27_event(4, true, 2000.0).expect("event creation is effect-free");
        let int = |field: u32| CGEvent::integer_value_field(Some(&event), CGEventField(field));
        let dbl = |field: u32| CGEvent::double_value_field(Some(&event), CGEventField(field));
        assert_eq!(int(55), 30);
        assert_eq!(int(110), 23);
        assert_eq!(int(123), 1);
        assert_eq!(int(132), 4);
        assert_eq!(int(134), 4);
        assert_eq!(dbl(124), 1.0);
        assert_eq!(dbl(129), 2000.0);
        assert_eq!(dbl(130), 0.0);
        assert_eq!(dbl(138), 3.0);
        // CGEvent stores the position field as f32 in its wire representation.
        assert_eq!(dbl(125), f64::from(0.1_f32));
        assert_ne!(dbl(169), 0.0);
        assert_eq!(
            CGEvent::integer_value_field(Some(&event), CGEventField::EventSourceUserData),
            crate::input_match::OWN_SWITCH_USER_DATA
        );

        // The pre-reconstruction serialized data carries the 4205 payload as
        // its last field: literal BE TLV header then the native-LE body.
        // This is the only exact check possible: CGEvent exposes no data-field
        // getter (integer/double value field 4205 both read 0), and
        // CGEventCreateData on the reconstructed event does not re-emit the
        // appended TLV, so the payload cannot be read back after
        // reconstruction. `CGEventCreateFromData` does recognize the field
        // (an unknown neighbour such as 4206 is rejected), so the payload is
        // not silently ignored at parse time.
        let serialized = serialized_gesture_27_event(4, true, 2000.0).expect("serialization");
        let tail = &serialized[serialized.len() - 100..];
        assert_eq!(&tail[..4], &[0x00, 0x60, 0x10, 0x6D]);
        let body = &tail[4..];
        assert_eq!(body.len(), 96);
        assert_eq!(&body[36..40], &[0, 0, 0, 0x04]);
        assert_eq!(
            i32::from_le_bytes(body[64..68].try_into().unwrap()),
            -65_536
        );
        assert_eq!(
            i32::from_le_bytes(body[84..88].try_into().unwrap()),
            -131_072_000
        );
    }
}
