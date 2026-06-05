// Vendored from wasmtime v45.0.0
// Source: examples/min-platform/embedding/wasmtime-platform.h
// Generated with cbindgen from crates/wasmtime/src/runtime/vm/sys/custom/capi.rs
//
// Platform support for Wasmtime's `no_std` build.
// Embedders must implement these symbols.

#ifndef _WASMTIME_PLATFORM_H
#define _WASMTIME_PLATFORM_H

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

#if defined(WASMTIME_VIRTUAL_MEMORY)
#define WASMTIME_PROT_READ (1 << 0)
#define WASMTIME_PROT_WRITE (1 << 1)
#define WASMTIME_PROT_EXEC (1 << 2)

typedef struct wasmtime_memory_image wasmtime_memory_image;
#endif

#if defined(WASMTIME_NATIVE_SIGNALS)
typedef void (*wasmtime_trap_handler_t)(uintptr_t ip,
                                        uintptr_t fp,
                                        bool has_faulting_addr,
                                        uintptr_t faulting_addr);
#endif

#ifdef __cplusplus
extern "C" {
#endif

#if defined(WASMTIME_VIRTUAL_MEMORY)
extern int32_t wasmtime_mmap_new(uintptr_t size, uint32_t prot_flags, uint8_t **ret);
extern int32_t wasmtime_mmap_remap(uint8_t *addr, uintptr_t size, uint32_t prot_flags);
extern int32_t wasmtime_munmap(uint8_t *ptr, uintptr_t size);
extern int32_t wasmtime_mprotect(uint8_t *ptr, uintptr_t size, uint32_t prot_flags);
extern uintptr_t wasmtime_page_size(void);
extern int32_t wasmtime_memory_image_new(const uint8_t *ptr,
                                         uintptr_t len,
                                         struct wasmtime_memory_image **ret);
extern int32_t wasmtime_memory_image_map_at(struct wasmtime_memory_image *image,
                                            uint8_t *addr,
                                            uintptr_t len);
extern void wasmtime_memory_image_free(struct wasmtime_memory_image *image);
#endif

#if defined(WASMTIME_NATIVE_SIGNALS)
extern int32_t wasmtime_init_traps(wasmtime_trap_handler_t handler);
#endif

extern uint8_t *wasmtime_tls_get(void);
extern void wasmtime_tls_set(uint8_t *ptr);

#if defined(WASMTIME_CUSTOM_SYNC)
extern void wasmtime_sync_lock_free(uintptr_t *lock);
extern void wasmtime_sync_lock_acquire(uintptr_t *lock);
extern void wasmtime_sync_lock_release(uintptr_t *lock);
extern void wasmtime_sync_rwlock_read(uintptr_t *lock);
extern void wasmtime_sync_rwlock_read_release(uintptr_t *lock);
extern void wasmtime_sync_rwlock_write(uintptr_t *lock);
extern void wasmtime_sync_rwlock_write_release(uintptr_t *lock);
extern void wasmtime_sync_rwlock_free(uintptr_t *lock);
#endif

#ifdef __cplusplus
}
#endif

#endif /* _WASMTIME_PLATFORM_H */
