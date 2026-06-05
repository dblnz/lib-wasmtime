#![no_std]

extern crate alloc;

pub use ukrust;
pub use wasmtime;

mod error;
mod platform;

pub use error::Error;

/// Run a precompiled WebAssembly module (.cwasm bytes).
///
/// Deserializes the module, instantiates it, and calls `_start`.
pub fn run_module(_precompiled: &[u8]) -> Result<(), Error> {
    // Phase 3: implement
    todo!()
}

/// Run a precompiled WebAssembly component (.cwasm bytes).
///
/// Deserializes the component, instantiates it, and calls the default export.
pub fn run_component(_precompiled: &[u8]) -> Result<(), Error> {
    // Phase 3: implement
    todo!()
}
