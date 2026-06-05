use core::fmt;

/// Errors returned by lib-ukwasmtime.
#[derive(Debug)]
pub enum Error {
    /// Failed to create the wasmtime Engine.
    EngineCreation,
    /// Failed to deserialize a precompiled module/component.
    Deserialization,
    /// Failed to instantiate or run the module/component.
    Execution,
    /// Wrapper for wasmtime::Error.
    Wasmtime(wasmtime::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::EngineCreation => write!(f, "failed to create wasmtime engine"),
            Error::Deserialization => write!(f, "failed to deserialize precompiled module"),
            Error::Execution => write!(f, "failed to execute module"),
            Error::Wasmtime(e) => write!(f, "wasmtime error: {e}"),
        }
    }
}

impl From<wasmtime::Error> for Error {
    fn from(e: wasmtime::Error) -> Self {
        Error::Wasmtime(e)
    }
}
