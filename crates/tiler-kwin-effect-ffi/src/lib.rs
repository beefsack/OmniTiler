//! KWin native-effect FFI policy: drag oracle verdicts only.
//!
//! Cargo workspace staticlib behind a minimal POD C ABI (AR10). No Qt/KWin
//! types cross this boundary; every exported callback catches panics before
//! returning. Rust owns drag verdict policy; C++ is only the QObject/D-Bus
//! boundary, native identity/signal observation, and outline/repaint shim.
//! Presentation rendering is derivation-free (see
//! kwin/native-effect/presentation_consumer.h, Qt only): the retired
//! group-highlight and visual-policy modules are deleted with their C++
//! consumers, keeping the drag oracle that the pending gesture
//! cancellation decision still requires.

#![deny(unsafe_code)]
// Preserve the existing C signatures and explicit panic-result branches at
// the FFI boundary; callers supply valid borrowed POD pointers.
#![allow(
    clippy::not_unsafe_ptr_arg_deref,
    clippy::manual_unwrap_or,
    clippy::manual_unwrap_or_default,
    clippy::needless_return,
    clippy::useless_vec
)]

pub mod drag_oracle;
