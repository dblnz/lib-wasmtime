/*
 * lib-ukwasmtime C API
 *
 * Create a wasmtime engine, load precompiled WebAssembly modules and
 * components (.cwasm), instantiate them, and call any exported function with
 * arbitrary numeric arguments from within a Unikraft unikernel.
 *
 * All module/component bytes must be precompiled with the same wasmtime
 * version and target architecture as this library.
 *
 * Typical use:
 *
 *     void *engine = ukwasmtime_engine_create();
 *     void *module = ukwasmtime_module_load(engine, bytes, len);
 *     void *inst   = ukwasmtime_instantiate_module(engine, module);
 *
 *     ukw_val_t args[2] = { UKW_VAL_I32(3), UKW_VAL_I32(4) };
 *     ukw_val_t res[1];
 *     size_t n;
 *     if (ukwasmtime_call(inst, "add", args, 2, res, 1, &n) == 0)
 *             printf("add = %d\n", res[0].of.i32);
 *
 *     ukwasmtime_instance_free(inst);
 *     ukwasmtime_module_destroy(module);
 *     ukwasmtime_engine_destroy(engine);
 */

#ifndef UKWASMTIME_H
#define UKWASMTIME_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ------------------------------------------------------------------ */
/* Values                                                             */
/* ------------------------------------------------------------------ */

/*
 * The supported value kinds. These are the four WebAssembly numeric types;
 * the tag selects the active member of ukw_val_t's union.
 */
typedef enum {
	UKW_I32 = 0,
	UKW_I64 = 1,
	UKW_F32 = 2,
	UKW_F64 = 3,
} ukw_valkind_t;

/*
 * A single WebAssembly value: a tag plus a union of the four scalar types.
 * The same type is used for both module (core) and component values.
 */
typedef struct {
	ukw_valkind_t kind;
	union {
		int32_t i32;
		int64_t i64;
		float   f32;
		double  f64;
	} of;
} ukw_val_t;

/* Convenience constructors so building argument arrays stays terse. */
#define UKW_VAL_I32(x) ((ukw_val_t){ .kind = UKW_I32, .of.i32 = (x) })
#define UKW_VAL_I64(x) ((ukw_val_t){ .kind = UKW_I64, .of.i64 = (x) })
#define UKW_VAL_F32(x) ((ukw_val_t){ .kind = UKW_F32, .of.f32 = (x) })
#define UKW_VAL_F64(x) ((ukw_val_t){ .kind = UKW_F64, .of.f64 = (x) })

/* ------------------------------------------------------------------ */
/* Engine                                                            */
/* ------------------------------------------------------------------ */

/*
 * Create a new wasmtime engine.
 * Returns an opaque engine handle, or NULL on failure.
 */
void *ukwasmtime_engine_create(void);

/*
 * Destroy an engine created with ukwasmtime_engine_create().
 */
void ukwasmtime_engine_destroy(void *engine);

/* ------------------------------------------------------------------ */
/* Loading                                                           */
/* ------------------------------------------------------------------ */

/*
 * Load a precompiled module (.cwasm bytes) into an engine.
 * Returns an opaque module handle, or NULL on failure.
 * The data pointer must remain valid only for the duration of this call.
 */
void *ukwasmtime_module_load(void *engine, const uint8_t *data, size_t len);

/*
 * Destroy a module loaded with ukwasmtime_module_load().
 */
void ukwasmtime_module_destroy(void *module);

/*
 * Load a precompiled component (.cwasm bytes) into an engine.
 * Returns an opaque component handle, or NULL on failure.
 */
void *ukwasmtime_component_load(void *engine, const uint8_t *data, size_t len);

/*
 * Destroy a component loaded with ukwasmtime_component_load().
 */
void ukwasmtime_component_destroy(void *component);

/* ------------------------------------------------------------------ */
/* Instantiation and calls                                           */
/* ------------------------------------------------------------------ */

/*
 * Instantiate a loaded module into a callable instance. A default
 * "env.print_i32" host import is provided and unknown imports trap.
 * Returns an opaque instance handle, or NULL on failure.
 */
void *ukwasmtime_instantiate_module(void *engine, void *module);

/*
 * Instantiate a loaded component into a callable instance.
 * Returns an opaque instance handle, or NULL on failure.
 */
void *ukwasmtime_instantiate_component(void *engine, void *component);

/*
 * Free an instance created by ukwasmtime_instantiate_module() or
 * ukwasmtime_instantiate_component().
 */
void ukwasmtime_instance_free(void *instance);

/*
 * Call exported function func_name on an instance (module or component).
 *
 * args/nargs are the inputs. results is a caller-allocated array of capacity
 * result_cap; on success the produced values are written there and *nresults
 * is set to their count. If the function produces more results than
 * result_cap, *nresults still reports the required count and the call returns
 * -2 without writing past the buffer.
 *
 * Returns 0 on success, -1 on a generic error, -2 on insufficient capacity.
 */
int ukwasmtime_call(void *instance, const char *func_name,
		    const ukw_val_t *args, size_t nargs,
		    ukw_val_t *results, size_t result_cap,
		    size_t *nresults);

#ifdef __cplusplus
}
#endif

#endif /* UKWASMTIME_H */
