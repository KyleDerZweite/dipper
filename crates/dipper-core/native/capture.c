#define _GNU_SOURCE
#include <pipewire/pipewire.h>
#include <spa/param/audio/format-utils.h>
#include <spa/param/buffers.h>
#include <spa/buffer/meta.h>
#include <stdatomic.h>
#include <stdint.h>
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
    _Atomic uint64_t read, written, accepted;
    _Atomic int fault;
};

struct dipper_ring *dipper_ring_new(uint64_t capacity) {
    if (capacity == 0 || capacity > SIZE_MAX / sizeof(float)) return NULL;
    struct dipper_ring *r = calloc(1, sizeof(*r));
    if (!r) return NULL;
    r->samples = calloc(capacity, sizeof(float));
    if (!r->samples || !atomic_is_lock_free(&r->read) || !atomic_is_lock_free(&r->fault)) {
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
int dipper_ring_push(struct dipper_ring *r, const float *samples, uint64_t count) {
    if (atomic_load_explicit(&r->fault, memory_order_acquire)) return 0;
    if (count == 0) return 1;
    uint64_t w = atomic_load_explicit(&r->written, memory_order_relaxed);
    uint64_t rd = atomic_load_explicit(&r->read, memory_order_acquire);
    if (count > r->capacity - (w - rd)) { dipper_ring_fail(r, 1); return 0; }
    uint64_t pos = w % r->capacity, first = count;
    if (first > r->capacity - pos) first = r->capacity - pos;
    memcpy(r->samples + pos, samples, first * sizeof(float));
    memcpy(r->samples, samples + first, (count - first) * sizeof(float));
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
    memcpy(dest + first, r->samples, (count - first) * sizeof(float));
    atomic_store_explicit(&r->read, rd + count, memory_order_release);
    return count;
}

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
};

static void process(void *userdata) {
    struct dipper_capture *c = userdata;
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
                if (h && size) {
                    if ((h->flags & (SPA_META_HEADER_FLAG_CORRUPTED | SPA_META_HEADER_FLAG_GAP)) ||
                        (c->have_sequence && ((h->flags & SPA_META_HEADER_FLAG_DISCONT) || h->seq < c->last_sequence)))
                        dipper_ring_fail(c->ring, 5);
                    c->last_sequence = h->seq; c->have_sequence = true;
                }
                if (size) dipper_ring_push(c->ring, (const float *)((uint8_t *)d->data + offset), size / sizeof(float));
            }
        }
    }
    pw_stream_queue_buffer(c->stream, buffer);
}
static void state_changed(void *userdata, enum pw_stream_state old, enum pw_stream_state state, const char *error) {
    (void)old; (void)error;
    struct dipper_capture *c = userdata;
    if (atomic_load_explicit(&c->closing, memory_order_acquire)) return;
    if (state == PW_STREAM_STATE_ERROR || (state == PW_STREAM_STATE_UNCONNECTED && c->had_stream))
        dipper_ring_fail(c->ring, 3);
    if (state == PW_STREAM_STATE_PAUSED || state == PW_STREAM_STATE_STREAMING) c->had_stream = true;
}
static void format_changed(void *userdata, uint32_t id, const struct spa_pod *param) {
    struct dipper_capture *c = userdata;
    if (id != SPA_PARAM_Format || !param) return;
    struct spa_audio_info_raw info = {0};
    if (spa_format_audio_raw_parse(param, &info) < 0 || info.format != SPA_AUDIO_FORMAT_F32_LE || info.rate != 48000 || info.channels != 1)
        dipper_ring_fail(c->ring, 4);
    else {
        uint8_t buffer[256]; struct spa_pod_builder b = SPA_POD_BUILDER_INIT(buffer, sizeof(buffer));
        const struct spa_pod *meta = spa_pod_builder_add_object(&b, SPA_TYPE_OBJECT_ParamMeta, SPA_PARAM_Meta,
            SPA_PARAM_META_type, SPA_POD_Id(SPA_META_Header), SPA_PARAM_META_size, SPA_POD_Int(sizeof(struct spa_meta_header)));
        if (pw_stream_update_params(c->stream, &meta, 1) < 0) dipper_ring_fail(c->ring, 3);
    }
}
static const struct pw_stream_events stream_events = { PW_VERSION_STREAM_EVENTS,
    .state_changed = state_changed, .param_changed = format_changed, .process = process };
static void global_added(void *userdata, uint32_t id, uint32_t permissions, const char *type, uint32_t version, const struct spa_dict *props) {
    (void)permissions; (void)version;
    struct dipper_capture *c = userdata;
    if (strcmp(type, PW_TYPE_INTERFACE_Node) || !props) return;
    const char *s = spa_dict_lookup(props, PW_KEY_OBJECT_SERIAL);
    if (s && strtoull(s, NULL, 10) == c->target_serial) c->target_id = id;
}
static void global_removed(void *userdata, uint32_t id) {
    struct dipper_capture *c = userdata;
    if (id == c->target_id && !atomic_load_explicit(&c->closing, memory_order_acquire)) dipper_ring_fail(c->ring, 2);
}
static const struct pw_registry_events registry_events = { PW_VERSION_REGISTRY_EVENTS, .global = global_added, .global_remove = global_removed };
static void core_error(void *userdata, uint32_t id, int seq, int res, const char *message) {
    (void)id; (void)seq; (void)res; (void)message;
    struct dipper_capture *c = userdata;
    if (!atomic_load_explicit(&c->closing, memory_order_acquire)) dipper_ring_fail(c->ring, 3);
}
static const struct pw_core_events core_events = { PW_VERSION_CORE_EVENTS, .error = core_error };

void dipper_capture_close(struct dipper_capture *c) {
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
    free(c);
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
