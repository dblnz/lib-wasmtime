// Wasmtime platform hooks for Unikraft
//
// Implements the C-ABI functions required by wasmtime's no_std mode when
// using custom-virtual-memory, custom-native-signals, and
// custom-sync-primitives features.
//
// Based on wasmtime v45.0.0 examples/min-platform/embedding/wasmtime-platform.c
// Adapted for the Unikraft unikernel environment.

#define _GNU_SOURCE

#include <errno.h>
#include <signal.h>
#include <stdatomic.h>
#include <stdint.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

#define WASMTIME_VIRTUAL_MEMORY
#define WASMTIME_NATIVE_SIGNALS
#define WASMTIME_CUSTOM_SYNC

#include "wasmtime-platform.h"

// ============================================================================
// Virtual Memory
// ============================================================================

static int wasmtime_to_mmap_prot(uint32_t prot_flags) {
    int flags = 0;
    if (prot_flags & WASMTIME_PROT_READ)
        flags |= PROT_READ;
    if (prot_flags & WASMTIME_PROT_WRITE)
        flags |= PROT_WRITE;
    if (prot_flags & WASMTIME_PROT_EXEC)
        flags |= PROT_EXEC;
    return flags;
}

int32_t wasmtime_mmap_new(uintptr_t size, uint32_t prot_flags, uint8_t **ret) {
    void *p = mmap(NULL, size, wasmtime_to_mmap_prot(prot_flags),
                   MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (p == MAP_FAILED)
        return errno;
    *ret = (uint8_t *)p;
    return 0;
}

int32_t wasmtime_mmap_remap(uint8_t *addr, uintptr_t size, uint32_t prot_flags) {
    void *p = mmap(addr, size, wasmtime_to_mmap_prot(prot_flags),
                   MAP_FIXED | MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (p == MAP_FAILED)
        return errno;
    return 0;
}

int32_t wasmtime_munmap(uint8_t *ptr, uintptr_t size) {
    if (munmap(ptr, size) != 0)
        return errno;
    return 0;
}

int32_t wasmtime_mprotect(uint8_t *ptr, uintptr_t size, uint32_t prot_flags) {
    if (mprotect(ptr, size, wasmtime_to_mmap_prot(prot_flags)) != 0)
        return errno;
    return 0;
}

uintptr_t wasmtime_page_size(void) {
    return (uintptr_t)sysconf(_SC_PAGESIZE);
}

// Memory images: not supported. Returning NULL is the valid "unsupported" path.
int32_t wasmtime_memory_image_new(const uint8_t *ptr, uintptr_t len,
                                  struct wasmtime_memory_image **ret) {
    *ret = NULL;
    return 0;
}

int32_t wasmtime_memory_image_map_at(struct wasmtime_memory_image *image,
                                     uint8_t *addr, uintptr_t len) {
    // Should never be called since wasmtime_memory_image_new always returns NULL
    abort();
}

void wasmtime_memory_image_free(struct wasmtime_memory_image *image) {
    if (image == NULL)
        return;
    // Should never be called with non-NULL since we never create images
    abort();
}

// ============================================================================
// Trap / Signal Handling
// ============================================================================

static wasmtime_trap_handler_t g_trap_handler = NULL;

static void trap_signal_handler(int signo, siginfo_t *info, void *context) {
    if (g_trap_handler == NULL)
        abort();

    uintptr_t ip, fp;

#if defined(__x86_64__)
    ucontext_t *cx = (ucontext_t *)context;
    ip = (uintptr_t)cx->uc_mcontext.gregs[REG_RIP];
    fp = (uintptr_t)cx->uc_mcontext.gregs[REG_RBP];
#elif defined(__aarch64__)
    ucontext_t *cx = (ucontext_t *)context;
    ip = (uintptr_t)cx->uc_mcontext.pc;
    fp = (uintptr_t)cx->uc_mcontext.regs[29];
#else
#error "Unsupported architecture for trap handling"
#endif

    bool has_faulting_addr = (signo == SIGSEGV);
    uintptr_t faulting_addr = 0;
    if (has_faulting_addr)
        faulting_addr = (uintptr_t)info->si_addr;

    g_trap_handler(ip, fp, has_faulting_addr, faulting_addr);

    // If the handler returned, the trap was not handled by wasmtime.
    // Reset to default behavior (will likely abort).
    signal(signo, SIG_DFL);
}

int32_t wasmtime_init_traps(wasmtime_trap_handler_t handler) {
    g_trap_handler = handler;

    struct sigaction sa;
    memset(&sa, 0, sizeof(sa));
    sa.sa_sigaction = trap_signal_handler;
    sa.sa_flags = SA_SIGINFO | SA_NODEFER;
    sigemptyset(&sa.sa_mask);

    if (sigaction(SIGILL, &sa, NULL) != 0)
        return errno;
    if (sigaction(SIGSEGV, &sa, NULL) != 0)
        return errno;
    if (sigaction(SIGFPE, &sa, NULL) != 0)
        return errno;
    return 0;
}

// ============================================================================
// TLS (Thread-Local Storage)
//
// Unikraft is single-threaded, so a simple static variable suffices.
// ============================================================================

static uint8_t *wasmtime_tls_value = NULL;

uint8_t *wasmtime_tls_get(void) {
    return wasmtime_tls_value;
}

void wasmtime_tls_set(uint8_t *ptr) {
    wasmtime_tls_value = ptr;
}

// ============================================================================
// Synchronization Primitives
//
// Unikraft runs single-threaded (cooperative scheduling). These use simple
// atomic flags for safety. In a truly single-threaded context they are
// effectively no-ops, but the atomic operations protect against any
// reentrancy from signal handlers or callbacks.
// ============================================================================

// Lock: use the uintptr_t directly as a flag (0 = unlocked, 1 = locked)

void wasmtime_sync_lock_acquire(uintptr_t *lock) {
    // In single-threaded Unikraft, this is essentially a no-op.
    // The atomic store provides a memory barrier for safety.
    atomic_store((atomic_uintptr_t *)lock, 1);
}

void wasmtime_sync_lock_release(uintptr_t *lock) {
    atomic_store((atomic_uintptr_t *)lock, 0);
}

void wasmtime_sync_lock_free(uintptr_t *lock) {
    *lock = 0;
}

// RwLock: use the uintptr_t as a counter.
// Positive = reader count, negative would mean write-locked.
// For single-threaded, we just track the state minimally.

void wasmtime_sync_rwlock_read(uintptr_t *lock) {
    atomic_fetch_add((atomic_uintptr_t *)lock, 1);
}

void wasmtime_sync_rwlock_read_release(uintptr_t *lock) {
    atomic_fetch_sub((atomic_uintptr_t *)lock, 1);
}

void wasmtime_sync_rwlock_write(uintptr_t *lock) {
    // In single-threaded mode, just mark as write-locked
    atomic_store((atomic_uintptr_t *)lock, (uintptr_t)-1);
}

void wasmtime_sync_rwlock_write_release(uintptr_t *lock) {
    atomic_store((atomic_uintptr_t *)lock, 0);
}

void wasmtime_sync_rwlock_free(uintptr_t *lock) {
    *lock = 0;
}
