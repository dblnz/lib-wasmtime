#![no_std]

pub use ukrust;
pub use wasmtime;

mod error;
mod platform;

pub use error::Error;
pub use wasmtime::component::Component;
pub use wasmtime::{Engine, Linker, Module, Store};

unsafe extern "C" {
    safe fn printf(fmt: *const core::ffi::c_char, ...) -> core::ffi::c_int;
}

/// Create a wasmtime Engine configured for precompiled-only (no_std) execution.
pub fn create_engine() -> Result<Engine, Error> {
    let config = wasmtime::Config::new();
    Engine::new(&config).map_err(|_| Error::EngineCreation)
}

/// Deserialize a precompiled module from `.cwasm` bytes.
///
/// # Safety
/// The bytes must originate from `wasmtime compile` with the same wasmtime
/// version (v45.0.0) and target architecture (x86_64).
pub fn load_module(engine: &Engine, precompiled: &[u8]) -> Result<Module, Error> {
    unsafe { Module::deserialize(engine, precompiled) }.map_err(|_| Error::Deserialization)
}

/// Deserialize a precompiled component from `.cwasm` bytes.
///
/// # Safety
/// The bytes must originate from `wasmtime compile --component` with the same
/// wasmtime version (v45.0.0) and target architecture (x86_64).
pub fn load_component(engine: &Engine, precompiled: &[u8]) -> Result<Component, Error> {
    unsafe { Component::deserialize(engine, precompiled) }.map_err(|_| Error::Deserialization)
}

/// Run a precompiled WebAssembly module (.cwasm bytes).
///
/// Deserializes the module, instantiates it with a basic `env.print_i32` host
/// function, and calls `_start`. Any unknown imports are stubbed with traps.
pub fn run_module(precompiled: &[u8]) -> Result<(), Error> {
    let engine = create_engine()?;
    let module = load_module(&engine, precompiled)?;
    let mut store: Store<()> = Store::new(&engine, ());

    let mut linker = Linker::<()>::new(&engine);
    linker
        .func_wrap("env", "print_i32", |val: i32| {
            printf(b"wasm> %d\n\0".as_ptr() as *const core::ffi::c_char, val);
        })
        .map_err(|e| Error::Wasmtime(e))?;

    linker
        .define_unknown_imports_as_traps(&module)
        .map_err(|e| Error::Wasmtime(e))?;

    let instance = linker
        .instantiate(&mut store, &module)
        .map_err(|e| Error::Wasmtime(e))?;

    let start = instance
        .get_typed_func::<(), ()>(&mut store, "_start")
        .map_err(|_| Error::Execution)?;

    start.call(&mut store, ()).map_err(|e| Error::Wasmtime(e))?;

    Ok(())
}

/// Run a precompiled WebAssembly component (.cwasm bytes).
///
/// Deserializes the component and instantiates it. For components with no
/// imports (e.g., a pure `add` function), this succeeds directly.
///
/// To call specific exports, use [`load_component`] with the wasmtime
/// component API directly.
pub fn run_component(precompiled: &[u8]) -> Result<(), Error> {
    let engine = create_engine()?;
    let component = load_component(&engine, precompiled)?;
    let mut store: Store<()> = Store::new(&engine, ());

    let linker = wasmtime::component::Linker::<()>::new(&engine);
    let _instance = linker
        .instantiate(&mut store, &component)
        .map_err(|e| Error::Wasmtime(e))?;

    Ok(())
}
