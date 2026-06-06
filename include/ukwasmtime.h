/*
 * lib-ukwasmtime C API
 *
 * Provides functions to create a wasmtime engine, load precompiled
 * WebAssembly modules (.cwasm), and execute them within a Unikraft
 * unikernel.
 *
 * All module/component bytes must be precompiled with the same
 * wasmtime version (v45.0.0) and target architecture.
 */

#ifndef UKWASMTIME_H
#define UKWASMTIME_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Create a new wasmtime engine.
 * Returns an opaque engine handle, or NULL on failure.
 */
void *ukwasmtime_engine_create(void);

/*
 * Destroy an engine created with ukwasmtime_engine_create().
 */
void ukwasmtime_engine_destroy(void *engine);

/*
 * Load a precompiled module (.cwasm bytes) into an engine.
 * Returns an opaque module handle, or NULL on failure.
 *
 * The data pointer must remain valid only for the duration of this call.
 */
void *ukwasmtime_module_load(void *engine, const uint8_t *data, size_t len);

/*
 * Destroy a module loaded with ukwasmtime_module_load().
 */
void ukwasmtime_module_destroy(void *module);

/*
 * Run a module's "_start" export.
 * Provides a default "env.print_i32" host function that prints to stdout.
 * Unknown imports are stubbed with traps.
 * Returns 0 on success, -1 on failure.
 */
int ukwasmtime_module_run(void *engine, void *module);

/*
 * Call a module's exported function with signature (i32, i32) -> i32.
 * The result is stored in *out.
 * Returns 0 on success, -1 on failure.
 */
int ukwasmtime_module_call_ii_i(void *engine, void *module,
				const char *func_name,
				int32_t a, int32_t b, int32_t *out);

/*
 * One-shot: load and run a module's "_start" from precompiled bytes.
 * Creates a temporary engine internally.
 * Returns 0 on success, -1 on failure.
 */
int ukwasmtime_run_module(const uint8_t *data, size_t len);

/*
 * One-shot: load and run a component from precompiled bytes.
 * Creates a temporary engine internally.
 * Returns 0 on success, -1 on failure.
 */
int ukwasmtime_run_component(const uint8_t *data, size_t len);

/*
 * Load a precompiled component (.cwasm bytes) into an engine.
 * Returns an opaque component handle, or NULL on failure.
 */
void *ukwasmtime_component_load(void *engine, const uint8_t *data, size_t len);

/*
 * Destroy a component loaded with ukwasmtime_component_load().
 */
void ukwasmtime_component_destroy(void *component);

/*
 * Call a component's exported function with signature (s32, s32) -> s32.
 * The result is stored in *out.
 * Returns 0 on success, -1 on failure.
 */
int ukwasmtime_component_call_ii_i(void *engine, void *component,
				   const char *func_name,
				   int32_t a, int32_t b, int32_t *out);

/*
 * Call a component's exported function with signature (s32) -> s32.
 * The result is stored in *out.
 * Returns 0 on success, -1 on failure.
 */
int ukwasmtime_component_call_i_i(void *engine, void *component,
				  const char *func_name,
				  int32_t a, int32_t *out);

#ifdef __cplusplus
}
#endif

#endif /* UKWASMTIME_H */
