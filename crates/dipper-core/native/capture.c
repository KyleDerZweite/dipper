#define _GNU_SOURCE
#include <pipewire/pipewire.h>
#include <spa/param/audio/format-utils.h>
#include <spa/param/buffers.h>
#include <spa/buffer/meta.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>

const char *dipper_pipewire_version(void) { return pw_get_library_version(); }
void dipper_pipewire_init(void) { pw_init(NULL, NULL); }

/* Single producer (PipeWire), single consumer (Journal writer). No callback
 * allocation, locks, system calls or logging. Whole buffers are accepted. */
struct dipper_ring {
    float *samples;
    uint64_t capacity;
    _Atomic uint64_t read, written, accepted, high_water;
    _Atomic int fault;
    _Atomic uint32_t level_milli;
};

struct dipper_ring *dipper_ring_new(uint64_t capacity) {
    if (capacity == 0 || capacity > SIZE_MAX / sizeof(float)) return NULL;
    struct dipper_ring *r = calloc(1, sizeof(*r));
    if (!r) return NULL;
    r->samples = calloc(capacity, sizeof(float));
    if (!r->samples || !atomic_is_lock_free(&r->read) || !atomic_is_lock_free(&r->fault) ||
        !atomic_is_lock_free(&r->high_water) || !atomic_is_lock_free(&r->level_milli)) {
        free(r->samples); free(r); return NULL;
    }
    r->capacity = capacity;
    return r;
}
void dipper_ring_free(struct dipper_ring *r) { if (r) { free(r->samples); free(r); } }
void dipper_ring_fail(struct dipper_ring *r, int code) {
    int expected = 0;
    atomic_compare_exchange_strong_explicit(&r->fault, &expected, code, memory_order_release, memory_order_relaxed);
}
int dipper_ring_fault(struct dipper_ring *r) { return atomic_load_explicit(&r->fault, memory_order_acquire); }
uint64_t dipper_ring_accepted(struct dipper_ring *r) { return atomic_load_explicit(&r->accepted, memory_order_acquire); }
uint64_t dipper_ring_high_water(struct dipper_ring *r) { return atomic_load_explicit(&r->high_water, memory_order_acquire); }
uint32_t dipper_ring_take_level_milli(struct dipper_ring *r) {
    return atomic_exchange_explicit(&r->level_milli, 0, memory_order_acq_rel);
}
int dipper_ring_push(struct dipper_ring *r, const float *samples, uint64_t count) {
    if (atomic_load_explicit(&r->fault, memory_order_acquire)) return 0;
    if (count == 0) return 1;
    uint64_t w = atomic_load_explicit(&r->written, memory_order_relaxed);
    uint64_t rd = atomic_load_explicit(&r->read, memory_order_acquire);
    uint64_t used = w - rd;
    if (count > r->capacity - used) { dipper_ring_fail(r, 1); return 0; }
    uint64_t pos = w % r->capacity, first = count;
    if (first > r->capacity - pos) first = r->capacity - pos;
    memcpy(r->samples + pos, samples, first * sizeof(float));
    if (count > first) {
        memcpy(r->samples, samples + first, (count - first) * sizeof(float));
    }
    float peak = 0.0f;
    for (uint64_t i = 0; i < count; ++i) {
        float s = samples[i];
        float a = s < 0.0f ? -s : s;
        if (a == a && a > peak) peak = a;
    }
    if (peak > 1.0f) peak = 1.0f;
    uint32_t milli = (uint32_t)(peak * 1000.0f);
    uint32_t cur_lvl = atomic_load_explicit(&r->level_milli, memory_order_relaxed);
    while (milli > cur_lvl &&
           !atomic_compare_exchange_weak_explicit(&r->level_milli, &cur_lvl, milli, memory_order_relaxed, memory_order_relaxed)) {}
    uint64_t new_used = used + count;
    uint64_t hw = atomic_load_explicit(&r->high_water, memory_order_relaxed);
    while (new_used > hw &&
           !atomic_compare_exchange_weak_explicit(&r->high_water, &hw, new_used, memory_order_relaxed, memory_order_relaxed)) {}
    atomic_store_explicit(&r->written, w + count, memory_order_release);
    atomic_fetch_add_explicit(&r->accepted, count, memory_order_release);
    return 1;
}
uint64_t dipper_ring_read(struct dipper_ring *r, float *dest, uint64_t capacity) {
    uint64_t rd = atomic_load_explicit(&r->read, memory_order_relaxed);
    uint64_t w = atomic_load_explicit(&r->written, memory_order_acquire);
    uint64_t count = w - rd; if (count > capacity) count = capacity;
    if (count == 0) return 0;
    uint64_t pos = rd % r->capacity, first = count;
    if (first > r->capacity - pos) first = r->capacity - pos;
    memcpy(dest, r->samples + pos, first * sizeof(float));
    if (count > first) {
        memcpy(dest + first, r->samples, (count - first) * sizeof(float));
    }
    atomic_store_explicit(&r->read, rd + count, memory_order_release);
    return count;
}

struct dipper_capture_stats {
    uint64_t callbacks;
    uint64_t empty_buffers;
    uint32_t discont_flags;
    uint32_t states_mask;
    uint32_t negotiated_rate;
    uint32_t negotiated_channels;
    int32_t core_error_res;
    char stream_error[256];
    char core_error[256];
};

struct dipper_capture {
    struct pw_thread_loop *loop;
    struct pw_context *context;
    struct pw_core *core;
    struct pw_registry *registry;
    struct pw_stream *stream;
    struct spa_hook registry_hook, stream_hook, core_hook;
    struct dipper_ring *ring;
    uint64_t target_serial;
    uint32_t target_id;
    _Atomic bool closing;
    bool have_sequence;
    uint64_t last_sequence;
    bool had_stream;
    _Atomic uint64_t callbacks;
    _Atomic uint64_t empty_buffers;
    _Atomic uint32_t discont_flags;
    _Atomic uint32_t states_mask;
    _Atomic uint32_t negotiated_rate;
    _Atomic uint32_t negotiated_channels;
    _Atomic int32_t core_error_res;
    char stream_error[256];
    char core_error[256];
};

static void copy_bounded(char *dst, size_t cap, const char *src) {
    if (cap == 0 || !src) return;
    snprintf(dst, cap, "%s", src);
}

static void process(void *userdata) {
    struct dipper_capture *c = userdata;
    atomic_fetch_add_explicit(&c->callbacks, 1, memory_order_relaxed);
    struct pw_buffer *buffer = pw_stream_dequeue_buffer(c->stream);
    if (!buffer) return;
    if (!atomic_load_explicit(&c->closing, memory_order_acquire) && !dipper_ring_fault(c->ring)) {
        struct spa_buffer *b = buffer->buffer;
        if (b->n_datas != 1 || !b->datas[0].data || !b->datas[0].chunk) {
            dipper_ring_fail(c->ring, 4);
        } else {
            struct spa_data *d = &b->datas[0];
            uint32_t offset = d->chunk->offset, size = d->chunk->size;
            if (offset > d->maxsize || size > d->maxsize - offset || size % sizeof(float) != 0 ||
                (d->chunk->stride != 0 && d->chunk->stride != sizeof(float)) || offset % sizeof(float) != 0) {
                dipper_ring_fail(c->ring, 4);
            } else {
                struct spa_meta_header *h = spa_buffer_find_meta_data(b, SPA_META_Header, sizeof(*h));
                if (h) {
                    if ((h->flags & (SPA_META_HEADER_FLAG_CORRUPTED | SPA_META_HEADER_FLAG_GAP)) ||
                        (c->have_sequence && ((h->flags & SPA_META_HEADER_FLAG_DISCONT) || h->seq < c->last_sequence))) {
                        uint32_t flags = h->flags ? (uint32_t)h->flags : 0x80000000u;
                        atomic_fetch_or_explicit(&c->discont_flags, flags, memory_order_relaxed);
                        dipper_ring_fail(c->ring, 5);
                    }
                    c->last_sequence = h->seq; c->have_sequence = true;
                }
                if (size == 0) {
                    atomic_fetch_add_explicit(&c->empty_buffers, 1, memory_order_relaxed);
                } else {
                    dipper_ring_push(c->ring, (const float *)((uint8_t *)d->data + offset), size / sizeof(float));
                }
            }
        }
    }
    pw_stream_queue_buffer(c->stream, buffer);
}
static void state_changed(void *userdata, enum pw_stream_state old, enum pw_stream_state state, const char *error) {
    (void)old;
    struct dipper_capture *c = userdata;
    uint32_t bit = 0;
    switch (state) {
        case PW_STREAM_STATE_ERROR: bit = 1u << 0; break;
        case PW_STREAM_STATE_UNCONNECTED: bit = 1u << 1; break;
        case PW_STREAM_STATE_CONNECTING: bit = 1u << 2; break;
        case PW_STREAM_STATE_PAUSED: bit = 1u << 3; break;
        case PW_STREAM_STATE_STREAMING: bit = 1u << 4; break;
    }
    atomic_fetch_or_explicit(&c->states_mask, bit, memory_order_relaxed);
    if (atomic_load_explicit(&c->closing, memory_order_acquire)) return;
    if (state == PW_STREAM_STATE_ERROR && error && !c->stream_error[0]) {
        copy_bounded(c->stream_error, sizeof(c->stream_error), error);
    }
    if (state == PW_STREAM_STATE_ERROR || (state == PW_STREAM_STATE_UNCONNECTED && c->had_stream))
        dipper_ring_fail(c->ring, 3);
    if (state == PW_STREAM_STATE_PAUSED || state == PW_STREAM_STATE_STREAMING) c->had_stream = true;
}
static void format_changed(void *userdata, uint32_t id, const struct spa_pod *param) {
    struct dipper_capture *c = userdata;
    if (id != SPA_PARAM_Format || !param) return;
    struct spa_audio_info_raw info = {0};
    int parsed = spa_format_audio_raw_parse(param, &info);
    if (parsed >= 0) {
        atomic_store_explicit(&c->negotiated_rate, info.rate, memory_order_relaxed);
        atomic_store_explicit(&c->negotiated_channels, info.channels, memory_order_relaxed);
    }
    if (parsed < 0 || info.format != SPA_AUDIO_FORMAT_F32_LE || info.rate != 48000 || info.channels != 1)
        dipper_ring_fail(c->ring, 4);
    else {
        uint8_t buffer[256]; struct spa_pod_builder b = SPA_POD_BUILDER_INIT(buffer, sizeof(buffer));
        const struct spa_pod *meta = spa_pod_builder_add_object(&b, SPA_TYPE_OBJECT_ParamMeta, SPA_PARAM_Meta,
            SPA_PARAM_META_type, SPA_POD_Id(SPA_META_Header), SPA_PARAM_META_size, SPA_POD_Int(sizeof(struct spa_meta_header)));
        if (!meta || pw_stream_update_params(c->stream, &meta, 1) < 0) dipper_ring_fail(c->ring, 3);
    }
}
static const struct pw_stream_events stream_events = { PW_VERSION_STREAM_EVENTS,
    .state_changed = state_changed, .param_changed = format_changed, .process = process };
static void global_added(void *userdata, uint32_t id, uint32_t permissions, const char *type, uint32_t version, const struct spa_dict *props) {
    (void)permissions; (void)version;
    struct dipper_capture *c = userdata;
    if (strcmp(type, PW_TYPE_INTERFACE_Node) || !props) return;
    const char *s = spa_dict_lookup(props, PW_KEY_OBJECT_SERIAL);
    if (s && *s != '\0') {
        errno = 0;
        char *end = NULL;
        unsigned long long serial = strtoull(s, &end, 10);
        if (end != s && *end == '\0' && errno == 0 && serial == c->target_serial) {
            c->target_id = id;
        }
    }
}
static void global_removed(void *userdata, uint32_t id) {
    struct dipper_capture *c = userdata;
    if (c->target_id != SPA_ID_INVALID && id == c->target_id && !atomic_load_explicit(&c->closing, memory_order_acquire)) {
        dipper_ring_fail(c->ring, 2);
    }
}
static const struct pw_registry_events registry_events = { PW_VERSION_REGISTRY_EVENTS, .global = global_added, .global_remove = global_removed };
static void core_error(void *userdata, uint32_t id, int seq, int res, const char *message) {
    (void)id; (void)seq;
    struct dipper_capture *c = userdata;
    if (!atomic_load_explicit(&c->closing, memory_order_acquire)) {
        atomic_store_explicit(&c->core_error_res, res, memory_order_relaxed);
        if (message && !c->core_error[0]) {
            copy_bounded(c->core_error, sizeof(c->core_error), message);
        }
        dipper_ring_fail(c->ring, 3);
    }
}
static const struct pw_core_events core_events = { PW_VERSION_CORE_EVENTS, .error = core_error };

void dipper_capture_close_with_stats(struct dipper_capture *c, struct dipper_capture_stats *out) {
    if (out) memset(out, 0, sizeof(*out));
    if (!c) return;
    atomic_store_explicit(&c->closing, true, memory_order_release);
    if (c->loop) {
        pw_thread_loop_lock(c->loop);
        if (c->stream) pw_stream_destroy(c->stream);
        if (c->registry) { spa_hook_remove(&c->registry_hook); pw_proxy_destroy((struct pw_proxy *)c->registry); }
        if (c->core) { spa_hook_remove(&c->core_hook); pw_core_disconnect(c->core); }
        if (c->context) pw_context_destroy(c->context);
        pw_thread_loop_unlock(c->loop);
        pw_thread_loop_stop(c->loop);
        pw_thread_loop_destroy(c->loop);
    }
    if (out) {
        out->callbacks = atomic_load_explicit(&c->callbacks, memory_order_relaxed);
        out->empty_buffers = atomic_load_explicit(&c->empty_buffers, memory_order_relaxed);
        out->discont_flags = atomic_load_explicit(&c->discont_flags, memory_order_relaxed);
        out->states_mask = atomic_load_explicit(&c->states_mask, memory_order_relaxed);
        out->negotiated_rate = atomic_load_explicit(&c->negotiated_rate, memory_order_relaxed);
        out->negotiated_channels = atomic_load_explicit(&c->negotiated_channels, memory_order_relaxed);
        out->core_error_res = atomic_load_explicit(&c->core_error_res, memory_order_relaxed);
        memcpy(out->stream_error, c->stream_error, sizeof(out->stream_error));
        memcpy(out->core_error, c->core_error, sizeof(out->core_error));
    }
    free(c);
}
void dipper_capture_close(struct dipper_capture *c) {
    dipper_capture_close_with_stats(c, NULL);
}
struct dipper_capture *dipper_capture_open(uint64_t target_serial, struct dipper_ring *ring) {
    struct dipper_capture *c = calloc(1, sizeof(*c));
    if (!c) return NULL;
    c->ring = ring; c->target_serial = target_serial; c->target_id = SPA_ID_INVALID;
    c->loop = pw_thread_loop_new("dipper-capture", NULL);
    if (!c->loop) goto error;
    if (pw_thread_loop_start(c->loop) < 0) goto error;
    pw_thread_loop_lock(c->loop);
    c->context = pw_context_new(pw_thread_loop_get_loop(c->loop), NULL, 0);
    if (!c->context) goto locked_error;
    c->core = pw_context_connect(c->context, NULL, 0);
    if (!c->core) goto locked_error;
    pw_core_add_listener(c->core, &c->core_hook, &core_events, c);
    c->registry = pw_core_get_registry(c->core, PW_VERSION_REGISTRY, 0);
    if (!c->registry) goto locked_error;
    pw_registry_add_listener(c->registry, &c->registry_hook, &registry_events, c);
    char serial[32]; snprintf(serial, sizeof(serial), "%llu", (unsigned long long)target_serial);
    c->stream = pw_stream_new(c->core, "Dipper capture", pw_properties_new(
        PW_KEY_MEDIA_TYPE, "Audio", PW_KEY_MEDIA_CATEGORY, "Capture", PW_KEY_MEDIA_ROLE, "Production",
        PW_KEY_TARGET_OBJECT, serial, PW_KEY_NODE_DONT_RECONNECT, "true", "node.dont-fallback", "true", "node.dont-move", "true", NULL));
    if (!c->stream) goto locked_error;
    pw_stream_add_listener(c->stream, &c->stream_hook, &stream_events, c);
    uint8_t buffer[1024]; struct spa_pod_builder builder = SPA_POD_BUILDER_INIT(buffer, sizeof(buffer));
    struct spa_audio_info_raw info = { .format = SPA_AUDIO_FORMAT_F32_LE, .rate = 48000, .channels = 1, .position = { SPA_AUDIO_CHANNEL_MONO } };
    const struct spa_pod *params[] = { spa_format_audio_raw_build(&builder, SPA_PARAM_EnumFormat, &info) };
    int result = pw_stream_connect(c->stream, PW_DIRECTION_INPUT, PW_ID_ANY,
        PW_STREAM_FLAG_AUTOCONNECT | PW_STREAM_FLAG_MAP_BUFFERS | PW_STREAM_FLAG_RT_PROCESS, params, 1);
    pw_thread_loop_unlock(c->loop);
    if (result < 0) goto error;
    return c;
locked_error:
    pw_thread_loop_unlock(c->loop);
error:
    dipper_ring_fail(ring, 3); dipper_capture_close(c); return NULL;
}
