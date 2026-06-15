#![no_std]

// Required when lib-ukwasmtime is built as a plain cargo dependency (e.g. by
// app-ukwasmtime), which does not pass `--extern alloc`. The standalone ukcargo
// build does pass it, making this declaration redundant there, so silence the
// rust-2018-idioms `unused_extern_crates` lint.
#[allow(unused_extern_crates)]
extern crate alloc;

pub use ukrust;
pub use wasmtime;

mod error;
mod platform;

pub use error::Error;
pub use wasmtime::component::Component;
pub use wasmtime::{Engine, Linker, Module, Store};

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ffi::{c_char, c_int, c_void};

use wasmtime::Val as CoreVal;
use wasmtime::component::Val as ComponentVal;

unsafe extern "C" {
    safe fn printf(fmt: *const c_char, ...) -> c_int;
}

// ============================================================================
// Engine / module / component building blocks
// ============================================================================

/// Create a wasmtime Engine configured for precompiled-only (no_std) execution.
pub fn create_engine() -> Result<Engine, Error> {
    let mut config = wasmtime::Config::new();
    config.wasm_component_model(true);
    // Configure wasmtime for the embedded memory model that the unikraft
    // wasmtime port (`platform.c`) can actually support. wasmtime's default
    // x86_64 tunables reserve a 4 GiB linear-memory region (+2 GiB grow-into
    // area) and rely on guard pages + faults for bounds checks. The port's
    // `wasmtime_mmap_new` is a thin wrapper over real unikraft
    // `mmap(MAP_ANONYMOUS)`, which cannot satisfy a multi-GiB reservation, so
    // instantiation fails with the defaults. (Contrast hyperlight-wasm, whose
    // mmap shim is a lazy, demand-paged allocator over a huge guest address
    // space and therefore runs wasmtime's defaults unchanged.) Each setting
    // below corresponds to a capability the port lacks:
    //   * no large mmap reservation  -> reservation(0) + small grow area
    //   * CoW images unsupported (`wasmtime_memory_image_new` returns NULL)
    //     -> memory_init_cow(false)
    //   * don't depend on unikraft delivering guard-page faults to the port's
    //     signal handler -> explicit bounds checks: signals_based_traps(false)
    //     + memory_guard_size(0)
    // These must match the `-O` flags `wasm/build.sh` passes to `wasmtime
    // compile`, otherwise precompiled `.cwasm` modules fail to instantiate.
    config.signals_based_traps(false);
    config.memory_reservation(0);
    config.memory_guard_size(0);
    config.memory_reservation_for_growth(1 << 20);
    config.memory_init_cow(false);
    Engine::new(&config).map_err(|_| Error::EngineCreation)
}

/// Deserialize a precompiled module from `.cwasm` bytes.
///
/// # Safety
/// The bytes must originate from `wasmtime compile` with the same wasmtime
/// version and target architecture (x86_64).
pub fn load_module(engine: &Engine, precompiled: &[u8]) -> Result<Module, Error> {
    unsafe { Module::deserialize(engine, precompiled) }.map_err(|_| Error::Deserialization)
}

/// Deserialize a precompiled component from `.cwasm` bytes.
///
/// # Safety
/// The bytes must originate from `wasmtime compile --component` with the same
/// wasmtime version and target architecture (x86_64).
pub fn load_component(engine: &Engine, precompiled: &[u8]) -> Result<Component, Error> {
    unsafe { Component::deserialize(engine, precompiled) }.map_err(|_| Error::Deserialization)
}

// ============================================================================
// WasmVal: the single scalar type shared by the Rust dynamic API, the C FFI,
// and both the core and component value worlds.
// ============================================================================

/// A WebAssembly numeric value. This is the common currency of the generic
/// call API: it maps to core `wasmtime::Val` for modules and to
/// `wasmtime::component::Val` for components, so callers use one type for both.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WasmVal {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
}

impl WasmVal {
    fn to_core(self) -> CoreVal {
        match self {
            WasmVal::I32(v) => CoreVal::I32(v),
            WasmVal::I64(v) => CoreVal::I64(v),
            WasmVal::F32(v) => CoreVal::F32(v.to_bits()),
            WasmVal::F64(v) => CoreVal::F64(v.to_bits()),
        }
    }

    fn from_core(v: &CoreVal) -> Result<WasmVal, Error> {
        Ok(match v {
            CoreVal::I32(x) => WasmVal::I32(*x),
            CoreVal::I64(x) => WasmVal::I64(*x),
            CoreVal::F32(bits) => WasmVal::F32(f32::from_bits(*bits)),
            CoreVal::F64(bits) => WasmVal::F64(f64::from_bits(*bits)),
            _ => return Err(Error::Type),
        })
    }

    fn to_component(self) -> ComponentVal {
        match self {
            WasmVal::I32(v) => ComponentVal::S32(v),
            WasmVal::I64(v) => ComponentVal::S64(v),
            WasmVal::F32(v) => ComponentVal::Float32(v),
            WasmVal::F64(v) => ComponentVal::Float64(v),
        }
    }

    fn from_component(v: &ComponentVal) -> Result<WasmVal, Error> {
        Ok(match v {
            ComponentVal::S32(x) => WasmVal::I32(*x),
            ComponentVal::U32(x) => WasmVal::I32(*x as i32),
            ComponentVal::S64(x) => WasmVal::I64(*x),
            ComponentVal::U64(x) => WasmVal::I64(*x as i64),
            ComponentVal::Float32(x) => WasmVal::F32(*x),
            ComponentVal::Float64(x) => WasmVal::F64(*x),
            _ => return Err(Error::Type),
        })
    }
}

// ============================================================================
// Instance: a ready-to-call module or component. Instantiate once, call any
// exported function any number of times.
// ============================================================================

enum Inner {
    Module {
        store: Store<()>,
        instance: wasmtime::Instance,
    },
    Component {
        store: Store<()>,
        instance: wasmtime::component::Instance,
    },
}

/// A callable WebAssembly instance. Bundles the store with the instantiated
/// module or component so exports can be invoked repeatedly without
/// re-instantiating.
pub struct Instance {
    inner: Inner,
}

impl Instance {
    /// Instantiate a precompiled module. A default `env.print_i32` host import
    /// is provided and any other unknown imports are stubbed with traps, so
    /// simple modules (including `_start`-style ones) instantiate out of the box.
    pub fn from_module(engine: &Engine, module: &Module) -> Result<Self, Error> {
        let mut store = Store::new(engine, ());
        let mut linker = Linker::<()>::new(engine);
        linker
            .func_wrap("env", "print_i32", |val: i32| {
                printf(b"wasm> %d\n\0".as_ptr() as *const c_char, val);
            })
            .map_err(Error::Wasmtime)?;
        linker
            .define_unknown_imports_as_traps(module)
            .map_err(Error::Wasmtime)?;
        let instance = linker
            .instantiate(&mut store, module)
            .map_err(Error::Wasmtime)?;
        Ok(Instance {
            inner: Inner::Module { store, instance },
        })
    }

    /// Instantiate a precompiled component with an empty linker (no imports).
    pub fn from_component(engine: &Engine, component: &Component) -> Result<Self, Error> {
        let mut store = Store::new(engine, ());
        let linker = wasmtime::component::Linker::<()>::new(engine);
        let instance = linker
            .instantiate(&mut store, component)
            .map_err(Error::Wasmtime)?;
        Ok(Instance {
            inner: Inner::Component { store, instance },
        })
    }

    /// Call exported function `name` with runtime-typed arguments, returning its
    /// results. Works for both modules and components and for any arity. The
    /// result vector length is derived from the function's own type.
    pub fn call_dyn(&mut self, name: &str, args: &[WasmVal]) -> Result<Vec<WasmVal>, Error> {
        match &mut self.inner {
            Inner::Module { store, instance } => {
                let func = instance
                    .get_func(&mut *store, name)
                    .ok_or(Error::Execution)?;
                let nresults = func.ty(&*store).results().len();

                let params: Vec<CoreVal> = args.iter().map(|v| v.to_core()).collect();
                let mut results = alloc::vec![CoreVal::I32(0); nresults];
                func.call(&mut *store, &params, &mut results)
                    .map_err(Error::Wasmtime)?;

                results.iter().map(WasmVal::from_core).collect()
            }
            Inner::Component { store, instance } => {
                let func = instance
                    .get_func(&mut *store, name)
                    .ok_or(Error::Execution)?;
                let nresults = func.ty(&*store).results().len();

                let params: Vec<ComponentVal> = args.iter().map(|v| v.to_component()).collect();
                let mut results = alloc::vec![ComponentVal::Bool(false); nresults];
                // `Func::call` also runs the component's post-return, if any.
                func.call(&mut *store, &params, &mut results)
                    .map_err(Error::Wasmtime)?;

                results.iter().map(WasmVal::from_component).collect()
            }
        }
    }

    /// Write `data` into the default linear memory at byte `offset`.
    /// Only supported for module instances (components have no raw memory).
    pub fn memory_write(&mut self, offset: usize, data: &[u8]) -> Result<(), Error> {
        match &mut self.inner {
            Inner::Module { store, instance } => {
                let mem = instance
                    .get_memory(&mut *store, "memory")
                    .ok_or(Error::Execution)?;
                mem.write(&mut *store, offset, data)
                    .map_err(|_| Error::Execution)
            }
            Inner::Component { .. } => Err(Error::Execution),
        }
    }

    /// Read `len` bytes from the default linear memory starting at `offset`.
    /// Only supported for module instances.
    pub fn memory_read(&mut self, offset: usize, len: usize) -> Result<Vec<u8>, Error> {
        match &mut self.inner {
            Inner::Module { store, instance } => {
                let mem = instance
                    .get_memory(&mut *store, "memory")
                    .ok_or(Error::Execution)?;
                let mut buf = alloc::vec![0u8; len];
                mem.read(&*store, offset, &mut buf)
                    .map_err(|_| Error::Execution)?;
                Ok(buf)
            }
            Inner::Component { .. } => Err(Error::Execution),
        }
    }

    /// Statically-typed call for module instances, reusing wasmtime's
    /// `WasmParams`/`WasmResults` so `P` and `R` can be tuples of scalars. This
    /// is the zero-conversion fast path for Rust callers; it returns
    /// [`Error::Execution`] for component instances (use [`call_dyn`] there).
    ///
    /// [`call_dyn`]: Instance::call_dyn
    pub fn call<P, R>(&mut self, name: &str, params: P) -> Result<R, Error>
    where
        P: wasmtime::WasmParams,
        R: wasmtime::WasmResults,
    {
        match &mut self.inner {
            Inner::Module { store, instance } => {
                let func = instance
                    .get_typed_func::<P, R>(&mut *store, name)
                    .map_err(|_| Error::Execution)?;
                func.call(&mut *store, params).map_err(Error::Wasmtime)
            }
            Inner::Component { .. } => Err(Error::Execution),
        }
    }
}

// ============================================================================
// C-compatible FFI API
// ============================================================================

/// Tag for [`UkwVal`], matching `ukw_valkind_t` in `ukwasmtime.h`.
pub const UKW_I32: c_int = 0;
pub const UKW_I64: c_int = 1;
pub const UKW_F32: c_int = 2;
pub const UKW_F64: c_int = 3;

/// Payload of [`UkwVal`]; mirrors the C `union` in `ukwasmtime.h`.
#[repr(C)]
pub union UkwPayload {
    pub i32_val: i32,
    pub i64_val: i64,
    pub f32_val: f32,
    pub f64_val: f64,
}

/// C value type: a tagged union of the four supported scalar types. Layout
/// matches `ukw_val_t` in `ukwasmtime.h`.
#[repr(C)]
pub struct UkwVal {
    pub kind: c_int,
    pub of: UkwPayload,
}

impl UkwVal {
    /// # Safety
    /// `kind` must correctly describe which union member is active.
    unsafe fn to_wasm(&self) -> Result<WasmVal, Error> {
        Ok(match self.kind {
            UKW_I32 => WasmVal::I32(unsafe { self.of.i32_val }),
            UKW_I64 => WasmVal::I64(unsafe { self.of.i64_val }),
            UKW_F32 => WasmVal::F32(unsafe { self.of.f32_val }),
            UKW_F64 => WasmVal::F64(unsafe { self.of.f64_val }),
            _ => return Err(Error::Type),
        })
    }

    fn from_wasm(v: WasmVal) -> UkwVal {
        match v {
            WasmVal::I32(x) => UkwVal {
                kind: UKW_I32,
                of: UkwPayload { i32_val: x },
            },
            WasmVal::I64(x) => UkwVal {
                kind: UKW_I64,
                of: UkwPayload { i64_val: x },
            },
            WasmVal::F32(x) => UkwVal {
                kind: UKW_F32,
                of: UkwPayload { f32_val: x },
            },
            WasmVal::F64(x) => UkwVal {
                kind: UKW_F64,
                of: UkwPayload { f64_val: x },
            },
        }
    }
}

/// Convert a NUL-terminated C string to a `&str` without allocating.
///
/// # Safety
/// `p` must point to a valid NUL-terminated, UTF-8 string that outlives `'a`.
unsafe fn cstr<'a>(p: *const c_char) -> &'a str {
    let mut len = 0usize;
    let mut q = p;
    while unsafe { *q } != 0 {
        len += 1;
        q = unsafe { q.add(1) };
    }
    unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(p as *const u8, len)) }
}

/// Create a new wasmtime engine. Returns an opaque handle, or NULL on failure.
#[unsafe(no_mangle)]
pub extern "C" fn ukwasmtime_engine_create() -> *mut c_void {
    match create_engine() {
        Ok(engine) => Box::into_raw(Box::new(engine)) as *mut c_void,
        Err(e) => {
            report("engine create failed", &e);
            core::ptr::null_mut()
        }
    }
}

/// Destroy an engine created with `ukwasmtime_engine_create`.
#[unsafe(no_mangle)]
pub extern "C" fn ukwasmtime_engine_destroy(engine: *mut c_void) {
    if !engine.is_null() {
        unsafe {
            let _ = Box::from_raw(engine as *mut Engine);
        }
    }
}

/// Load a precompiled module (.cwasm bytes). Returns an opaque module handle,
/// or NULL on failure. The `data` pointer need only be valid for this call.
#[unsafe(no_mangle)]
pub extern "C" fn ukwasmtime_module_load(
    engine: *mut c_void,
    data: *const u8,
    len: usize,
) -> *mut c_void {
    if engine.is_null() || data.is_null() {
        return core::ptr::null_mut();
    }
    let engine = unsafe { &*(engine as *const Engine) };
    let bytes = unsafe { core::slice::from_raw_parts(data, len) };
    match load_module(engine, bytes) {
        Ok(module) => Box::into_raw(Box::new(module)) as *mut c_void,
        Err(e) => {
            report("module load failed", &e);
            core::ptr::null_mut()
        }
    }
}

/// Destroy a module loaded with `ukwasmtime_module_load`.
#[unsafe(no_mangle)]
pub extern "C" fn ukwasmtime_module_destroy(module: *mut c_void) {
    if !module.is_null() {
        unsafe {
            let _ = Box::from_raw(module as *mut Module);
        }
    }
}

/// Load a precompiled component (.cwasm bytes). Returns an opaque component
/// handle, or NULL on failure.
#[unsafe(no_mangle)]
pub extern "C" fn ukwasmtime_component_load(
    engine: *mut c_void,
    data: *const u8,
    len: usize,
) -> *mut c_void {
    if engine.is_null() || data.is_null() {
        return core::ptr::null_mut();
    }
    let engine = unsafe { &*(engine as *const Engine) };
    let bytes = unsafe { core::slice::from_raw_parts(data, len) };
    match load_component(engine, bytes) {
        Ok(component) => Box::into_raw(Box::new(component)) as *mut c_void,
        Err(e) => {
            report("component load failed", &e);
            core::ptr::null_mut()
        }
    }
}

/// Destroy a component loaded with `ukwasmtime_component_load`.
#[unsafe(no_mangle)]
pub extern "C" fn ukwasmtime_component_destroy(component: *mut c_void) {
    if !component.is_null() {
        unsafe {
            let _ = Box::from_raw(component as *mut Component);
        }
    }
}

/// Instantiate a loaded module into a callable instance. Returns an opaque
/// instance handle, or NULL on failure.
#[unsafe(no_mangle)]
pub extern "C" fn ukwasmtime_instantiate_module(
    engine: *mut c_void,
    module: *mut c_void,
) -> *mut c_void {
    if engine.is_null() || module.is_null() {
        return core::ptr::null_mut();
    }
    let engine = unsafe { &*(engine as *const Engine) };
    let module = unsafe { &*(module as *const Module) };
    match Instance::from_module(engine, module) {
        Ok(inst) => Box::into_raw(Box::new(inst)) as *mut c_void,
        Err(e) => {
            report("module instantiate failed", &e);
            core::ptr::null_mut()
        }
    }
}

/// Instantiate a loaded component into a callable instance. Returns an opaque
/// instance handle, or NULL on failure.
#[unsafe(no_mangle)]
pub extern "C" fn ukwasmtime_instantiate_component(
    engine: *mut c_void,
    component: *mut c_void,
) -> *mut c_void {
    if engine.is_null() || component.is_null() {
        return core::ptr::null_mut();
    }
    let engine = unsafe { &*(engine as *const Engine) };
    let component = unsafe { &*(component as *const Component) };
    match Instance::from_component(engine, component) {
        Ok(inst) => Box::into_raw(Box::new(inst)) as *mut c_void,
        Err(e) => {
            report("component instantiate failed", &e);
            core::ptr::null_mut()
        }
    }
}

/// Free an instance created with `ukwasmtime_instantiate_module` or
/// `ukwasmtime_instantiate_component`.
#[unsafe(no_mangle)]
pub extern "C" fn ukwasmtime_instance_free(instance: *mut c_void) {
    if !instance.is_null() {
        unsafe {
            let _ = Box::from_raw(instance as *mut Instance);
        }
    }
}

/// Call exported function `func_name` on `instance` (module or component).
///
/// `args`/`nargs` are the inputs. `results` is a caller-allocated array of
/// capacity `result_cap`; on success the produced values are written there and
/// `*nresults` is set to their count. If the function produces more results
/// than `result_cap`, `*nresults` still reports the required count and the call
/// returns -2 without writing past the buffer.
///
/// Returns 0 on success, -1 on a generic error, -2 on insufficient capacity.
#[unsafe(no_mangle)]
pub extern "C" fn ukwasmtime_call(
    instance: *mut c_void,
    func_name: *const c_char,
    args: *const UkwVal,
    nargs: usize,
    results: *mut UkwVal,
    result_cap: usize,
    nresults: *mut usize,
) -> c_int {
    if instance.is_null() || func_name.is_null() {
        return -1;
    }
    let inst = unsafe { &mut *(instance as *mut Instance) };
    let name = unsafe { cstr(func_name) };

    let mut wargs: Vec<WasmVal> = Vec::with_capacity(nargs);
    if nargs > 0 {
        if args.is_null() {
            return -1;
        }
        let slice = unsafe { core::slice::from_raw_parts(args, nargs) };
        for a in slice {
            match unsafe { a.to_wasm() } {
                Ok(v) => wargs.push(v),
                Err(_) => return -1,
            }
        }
    }

    let produced = match inst.call_dyn(name, &wargs) {
        Ok(r) => r,
        Err(e) => {
            report("call failed", &e);
            return -1;
        }
    };

    if !nresults.is_null() {
        unsafe { *nresults = produced.len() };
    }
    if produced.len() > result_cap {
        return -2;
    }
    if !results.is_null() && !produced.is_empty() {
        let out = unsafe { core::slice::from_raw_parts_mut(results, produced.len()) };
        for (slot, v) in out.iter_mut().zip(produced.iter()) {
            *slot = UkwVal::from_wasm(*v);
        }
    }
    0
}

/// Print a diagnostic for an error to the console.
fn report(context: &str, err: &Error) {
    use alloc::string::ToString;
    let msg = err.to_string();
    printf(
        b"ERROR [ukwasmtime]: %.*s: %.*s\n\0".as_ptr() as *const c_char,
        context.len() as c_int,
        context.as_ptr() as *const c_char,
        msg.len() as c_int,
        msg.as_ptr() as *const c_char,
    );
}
