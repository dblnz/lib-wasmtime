#![no_std]

pub use ukrust;
pub use wasmtime;

mod error;
mod platform;

pub use error::Error;
pub use wasmtime::component::Component;
pub use wasmtime::{Engine, Linker, Module, Store};

use core::ffi::{c_char, c_int, c_void};

unsafe extern "C" {
    safe fn printf(fmt: *const c_char, ...) -> c_int;
}

/// Create a wasmtime Engine configured for precompiled-only (no_std) execution.
pub fn create_engine() -> Result<Engine, Error> {
    let mut config = wasmtime::Config::new();
    config.wasm_component_model(true);
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
            printf(b"wasm> %d\n\0".as_ptr() as *const c_char, val);
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

// ============================================================================
// C-compatible FFI API
// ============================================================================

/// Opaque handle to a wasmtime Engine.
struct EngineHandle {
    engine: Engine,
}

/// Opaque handle to a loaded Module.
struct ModuleHandle {
    module: Module,
}

/// Opaque handle to a loaded Component.
struct ComponentHandle {
    component: Component,
}

/// Create a new wasmtime engine. Returns an opaque pointer, or NULL on failure.
#[unsafe(no_mangle)]
pub extern "C" fn ukwasmtime_engine_create() -> *mut c_void {
    match create_engine() {
        Ok(engine) => {
            let handle = alloc::boxed::Box::new(EngineHandle { engine });
            alloc::boxed::Box::into_raw(handle) as *mut c_void
        }
        Err(e) => {
            use alloc::string::ToString;
            let msg = e.to_string();
            printf(
                b"ERROR [ukwasmtime]: engine create failed: %.*s\n\0".as_ptr() as *const c_char,
                msg.len() as c_int,
                msg.as_ptr() as *const c_char,
            );
            core::ptr::null_mut()
        }
    }
}

/// Destroy an engine created with `ukwasmtime_engine_create`.
#[unsafe(no_mangle)]
pub extern "C" fn ukwasmtime_engine_destroy(engine: *mut c_void) {
    if !engine.is_null() {
        unsafe {
            let _ = alloc::boxed::Box::from_raw(engine as *mut EngineHandle);
        }
    }
}

/// Load a precompiled module (.cwasm bytes) into an engine.
/// Returns an opaque module pointer, or NULL on failure.
#[unsafe(no_mangle)]
pub extern "C" fn ukwasmtime_module_load(
    engine: *mut c_void,
    data: *const u8,
    len: usize,
) -> *mut c_void {
    if engine.is_null() || data.is_null() {
        return core::ptr::null_mut();
    }
    let eh = unsafe { &*(engine as *const EngineHandle) };
    let bytes = unsafe { core::slice::from_raw_parts(data, len) };

    match unsafe { Module::deserialize(&eh.engine, bytes) } {
        Ok(module) => {
            let handle = alloc::boxed::Box::new(ModuleHandle { module });
            alloc::boxed::Box::into_raw(handle) as *mut c_void
        }
        Err(e) => {
            let msg = alloc::format!("{:?}", e);
            printf(
                b"ERROR [ukwasmtime]: module deserialize failed: %.*s\n\0".as_ptr()
                    as *const c_char,
                msg.len() as c_int,
                msg.as_ptr() as *const c_char,
            );
            core::ptr::null_mut()
        }
    }
}

/// Destroy a module loaded with `ukwasmtime_module_load`.
#[unsafe(no_mangle)]
pub extern "C" fn ukwasmtime_module_destroy(module: *mut c_void) {
    if !module.is_null() {
        unsafe {
            let _ = alloc::boxed::Box::from_raw(module as *mut ModuleHandle);
        }
    }
}

/// Run a module's `_start` export. Provides `env.print_i32` as a host import.
/// Returns 0 on success, -1 on failure.
#[unsafe(no_mangle)]
pub extern "C" fn ukwasmtime_module_run(engine: *mut c_void, module: *mut c_void) -> c_int {
    if engine.is_null() || module.is_null() {
        return -1;
    }
    let eh = unsafe { &*(engine as *const EngineHandle) };
    let mh = unsafe { &*(module as *const ModuleHandle) };

    let mut store: Store<()> = Store::new(&eh.engine, ());
    let mut linker = Linker::<()>::new(&eh.engine);

    if linker
        .func_wrap("env", "print_i32", |val: i32| {
            printf(b"wasm> %d\n\0".as_ptr() as *const c_char, val);
        })
        .is_err()
    {
        return -1;
    }

    let _ = linker.define_unknown_imports_as_traps(&mh.module);

    let instance = match linker.instantiate(&mut store, &mh.module) {
        Ok(i) => i,
        Err(_) => return -1,
    };

    match instance.get_typed_func::<(), ()>(&mut store, "_start") {
        Ok(start) => {
            if start.call(&mut store, ()).is_err() {
                return -1;
            }
        }
        Err(_) => return -1,
    }

    0
}

/// Run a module's exported function that takes (i32, i32) and returns i32.
/// Returns the result via `*out`. Returns 0 on success, -1 on failure.
#[unsafe(no_mangle)]
pub extern "C" fn ukwasmtime_module_call_ii_i(
    engine: *mut c_void,
    module: *mut c_void,
    func_name: *const c_char,
    a: i32,
    b: i32,
    out: *mut i32,
) -> c_int {
    if engine.is_null() || module.is_null() || func_name.is_null() || out.is_null() {
        return -1;
    }
    let eh = unsafe { &*(engine as *const EngineHandle) };
    let mh = unsafe { &*(module as *const ModuleHandle) };

    // Convert C string to Rust &str
    let name = unsafe {
        let mut len = 0usize;
        let mut p = func_name;
        while *p != 0 {
            len += 1;
            p = p.add(1);
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(func_name as *const u8, len))
    };

    let mut store: Store<()> = Store::new(&eh.engine, ());
    let linker = Linker::<()>::new(&eh.engine);

    let instance = match linker.instantiate(&mut store, &mh.module) {
        Ok(i) => i,
        Err(_) => return -1,
    };

    let func = match instance.get_typed_func::<(i32, i32), i32>(&mut store, name) {
        Ok(f) => f,
        Err(_) => return -1,
    };

    match func.call(&mut store, (a, b)) {
        Ok(result) => {
            unsafe { *out = result };
            0
        }
        Err(_) => -1,
    }
}

/// One-shot: load and run a module's `_start` from precompiled bytes.
/// Returns 0 on success, -1 on failure.
#[unsafe(no_mangle)]
pub extern "C" fn ukwasmtime_run_module(data: *const u8, len: usize) -> c_int {
    if data.is_null() {
        return -1;
    }
    let bytes = unsafe { core::slice::from_raw_parts(data, len) };
    if run_module(bytes).is_ok() { 0 } else { -1 }
}

/// One-shot: load and run a component from precompiled bytes.
/// Returns 0 on success, -1 on failure.
#[unsafe(no_mangle)]
pub extern "C" fn ukwasmtime_run_component(data: *const u8, len: usize) -> c_int {
    if data.is_null() {
        return -1;
    }
    let bytes = unsafe { core::slice::from_raw_parts(data, len) };
    if run_component(bytes).is_ok() { 0 } else { -1 }
}

// ============================================================================
// Component C API
// ============================================================================

/// Load a precompiled component (.cwasm bytes) into an engine.
/// Returns an opaque component handle, or NULL on failure.
#[unsafe(no_mangle)]
pub extern "C" fn ukwasmtime_component_load(
    engine: *mut c_void,
    data: *const u8,
    len: usize,
) -> *mut c_void {
    if engine.is_null() || data.is_null() {
        return core::ptr::null_mut();
    }
    let eh = unsafe { &*(engine as *const EngineHandle) };
    let bytes = unsafe { core::slice::from_raw_parts(data, len) };

    match unsafe { Component::deserialize(&eh.engine, bytes) } {
        Ok(component) => {
            let handle = alloc::boxed::Box::new(ComponentHandle { component });
            alloc::boxed::Box::into_raw(handle) as *mut c_void
        }
        Err(e) => {
            let msg = alloc::format!("{:?}", e);
            printf(
                b"ERROR [ukwasmtime]: component deserialize failed: %.*s\n\0".as_ptr()
                    as *const c_char,
                msg.len() as c_int,
                msg.as_ptr() as *const c_char,
            );
            core::ptr::null_mut()
        }
    }
}

/// Destroy a component loaded with `ukwasmtime_component_load`.
#[unsafe(no_mangle)]
pub extern "C" fn ukwasmtime_component_destroy(component: *mut c_void) {
    if !component.is_null() {
        unsafe {
            let _ = alloc::boxed::Box::from_raw(component as *mut ComponentHandle);
        }
    }
}

/// Instantiate a component and call an exported function: (s32, s32) -> s32.
/// Returns 0 on success, -1 on failure.
#[unsafe(no_mangle)]
pub extern "C" fn ukwasmtime_component_call_ii_i(
    engine: *mut c_void,
    component: *mut c_void,
    func_name: *const c_char,
    a: i32,
    b: i32,
    out: *mut i32,
) -> c_int {
    if engine.is_null() || component.is_null() || func_name.is_null() || out.is_null() {
        return -1;
    }
    let eh = unsafe { &*(engine as *const EngineHandle) };
    let ch = unsafe { &*(component as *const ComponentHandle) };

    let name = unsafe {
        let mut len = 0usize;
        let mut p = func_name;
        while *p != 0 {
            len += 1;
            p = p.add(1);
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(func_name as *const u8, len))
    };

    let mut store: Store<()> = Store::new(&eh.engine, ());
    let linker = wasmtime::component::Linker::<()>::new(&eh.engine);

    let instance = match linker.instantiate(&mut store, &ch.component) {
        Ok(i) => i,
        Err(e) => {
            let msg = alloc::format!("{:?}", e);
            printf(
                b"ERROR [ukwasmtime]: component instantiate failed: %.*s\n\0".as_ptr()
                    as *const c_char,
                msg.len() as c_int,
                msg.as_ptr() as *const c_char,
            );
            return -1;
        }
    };

    let func = match instance.get_typed_func::<(i32, i32), (i32,)>(&mut store, name) {
        Ok(f) => f,
        Err(e) => {
            let msg = alloc::format!("{:?}", e);
            printf(
                b"ERROR [ukwasmtime]: component get func failed: %.*s\n\0".as_ptr()
                    as *const c_char,
                msg.len() as c_int,
                msg.as_ptr() as *const c_char,
            );
            return -1;
        }
    };

    match func.call(&mut store, (a, b)) {
        Ok((result,)) => {
            unsafe { *out = result };
            0
        }
        Err(_) => -1,
    }
}

/// Instantiate a component and call an exported function: (s32) -> s32.
/// Returns 0 on success, -1 on failure.
#[unsafe(no_mangle)]
pub extern "C" fn ukwasmtime_component_call_i_i(
    engine: *mut c_void,
    component: *mut c_void,
    func_name: *const c_char,
    a: i32,
    out: *mut i32,
) -> c_int {
    if engine.is_null() || component.is_null() || func_name.is_null() || out.is_null() {
        return -1;
    }
    let eh = unsafe { &*(engine as *const EngineHandle) };
    let ch = unsafe { &*(component as *const ComponentHandle) };

    let name = unsafe {
        let mut len = 0usize;
        let mut p = func_name;
        while *p != 0 {
            len += 1;
            p = p.add(1);
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(func_name as *const u8, len))
    };

    let mut store: Store<()> = Store::new(&eh.engine, ());
    let linker = wasmtime::component::Linker::<()>::new(&eh.engine);

    let instance = match linker.instantiate(&mut store, &ch.component) {
        Ok(i) => i,
        Err(e) => {
            let msg = alloc::format!("{:?}", e);
            printf(
                b"ERROR [ukwasmtime]: component instantiate failed: %.*s\n\0".as_ptr()
                    as *const c_char,
                msg.len() as c_int,
                msg.as_ptr() as *const c_char,
            );
            return -1;
        }
    };

    let func = match instance.get_typed_func::<(i32,), (i32,)>(&mut store, name) {
        Ok(f) => f,
        Err(e) => {
            let msg = alloc::format!("{:?}", e);
            printf(
                b"ERROR [ukwasmtime]: component get func failed: %.*s\n\0".as_ptr()
                    as *const c_char,
                msg.len() as c_int,
                msg.as_ptr() as *const c_char,
            );
            return -1;
        }
    };

    match func.call(&mut store, (a,)) {
        Ok((result,)) => {
            unsafe { *out = result };
            0
        }
        Err(_) => -1,
    }
}
