// Wasmtime platform hooks for Unikraft.
//
// The C-ABI functions required by wasmtime's no_std mode are implemented in
// `platform.c` (at the library root). This file exists as documentation.
//
// The following function groups are provided:
//
// Virtual memory (custom-virtual-memory):
//   wasmtime_mmap_new, wasmtime_mmap_remap, wasmtime_munmap,
//   wasmtime_mprotect, wasmtime_page_size
//   wasmtime_memory_image_new (returns NULL — unsupported),
//   wasmtime_memory_image_map_at, wasmtime_memory_image_free
//
// Trap handling (custom-native-signals):
//   wasmtime_init_traps — installs signal handlers for SIGILL, SIGSEGV, SIGFPE
//
// TLS (always required):
//   wasmtime_tls_get, wasmtime_tls_set — static variable (single-threaded)
//
// Sync primitives (custom-sync-primitives):
//   wasmtime_sync_lock_acquire/release/free — atomic flag
//   wasmtime_sync_rwlock_read/read_release/write/write_release/free — atomic counter
