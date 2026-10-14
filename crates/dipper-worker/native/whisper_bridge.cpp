#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <exception>
#include <string>
#include <stdexcept>

#ifdef DIPPER_HAS_WHISPER
#include "whisper.h"
#include "ggml-backend.h"
#endif

struct DipperGpu {
    char backend[64];
    char name[128];
    char description[512];
    char pci_id[128];
    uint32_t gpu_index;
};

static void copy_text(char * target, size_t capacity, const char * source) {
    if (capacity) std::snprintf(target, capacity, "%s", source ? source : "");
}

#ifdef DIPPER_HAS_WHISPER
struct DipperContext {
    whisper_context * native = nullptr;
    bool gpu_failed = false;
    bool gpu_selected = false;
};

static void log_to_stderr(enum ggml_log_level, const char * text, void * opaque) {
    // The pinned loader can continue with CPU if GPU initialization fails.
    // Treat that path as failure rather than presenting it as the chosen baseline.
    auto * context = static_cast<DipperContext *>(opaque);
    if (text && std::strstr(text, "whisper_backend_init_gpu: using ")) context->gpu_selected = true;
    if (text && (std::strstr(text, "whisper_backend_init_gpu: failed") ||
                 std::strstr(text, "whisper_backend_init_gpu: no GPU found"))) context->gpu_failed = true;
    if (text) std::fputs(text, stderr);
}
#endif

extern "C" void * dipper_whisper_prepare(const char * model, const char * required_gpu,
                                          DipperGpu * gpu, char * error, size_t error_size) {
#ifdef DIPPER_HAS_WHISPER
    auto * context = new DipperContext();
    try {
        whisper_log_set(log_to_stderr, context);
        ggml_log_set(log_to_stderr, context);
        ggml_backend_load_all();
        int gpu_index = 0;
        int selected = -1;
        for (size_t index = 0; index < ggml_backend_dev_count(); ++index) {
            auto * device = ggml_backend_dev_get(index);
            ggml_backend_dev_props properties;
            ggml_backend_dev_get_props(device, &properties);
            if (properties.type != GGML_BACKEND_DEVICE_TYPE_GPU &&
                properties.type != GGML_BACKEND_DEVICE_TYPE_IGPU) continue;
            const char * backend = ggml_backend_reg_name(ggml_backend_dev_backend_reg(device));
            if (backend && std::strcmp(backend, "Vulkan") == 0 && properties.description &&
                std::strstr(properties.description, required_gpu)) {
                if (selected >= 0) throw std::runtime_error("required GPU identity is ambiguous");
                selected = gpu_index;
                copy_text(gpu->backend, sizeof(gpu->backend), backend);
                copy_text(gpu->name, sizeof(gpu->name), properties.name);
                copy_text(gpu->description, sizeof(gpu->description), properties.description);
                copy_text(gpu->pci_id, sizeof(gpu->pci_id), properties.device_id);
                gpu->gpu_index = static_cast<uint32_t>(gpu_index);
            }
            ++gpu_index;
        }
        if (selected < 0) throw std::runtime_error("required RX 7700S Vulkan device is unavailable");
        auto parameters = whisper_context_default_params();
        parameters.use_gpu = true;
        parameters.gpu_device = selected;
        context->native = whisper_init_from_file_with_params(model, parameters);
        if (!context->native) throw std::runtime_error("Whisper initialization failed");
        if (!context->gpu_selected || context->gpu_failed) throw std::runtime_error("Vulkan initialization failed; CPU fallback rejected");
        if (whisper_model_n_mels(context->native) != 128 ||
            whisper_model_n_audio_layer(context->native) != 32 ||
            whisper_model_n_text_layer(context->native) != 32 ||
            whisper_model_n_audio_state(context->native) != 1280 ||
            whisper_model_n_text_state(context->native) != 1280 ||
            whisper_model_ftype(context->native) != GGML_FTYPE_MOSTLY_F16)
            throw std::runtime_error("configured model is not the full float16 Whisper large-v3 baseline");
        return context;
    } catch (const std::exception & exception) {
        copy_text(error, error_size, exception.what());
    } catch (...) { copy_text(error, error_size, "native preparation failed with unknown exception"); }
    if (context->native) whisper_free(context->native);
    whisper_log_set(nullptr, nullptr);
    ggml_log_set(nullptr, nullptr);
    delete context;
#else
    (void)model; (void)required_gpu; (void)gpu;
    copy_text(error, error_size, "native runtime unavailable; build with DIPPER_WHISPER_DIR at the pinned Vulkan checkout");
#endif
    return nullptr;
}

extern "C" int dipper_whisper_recognize(void * opaque, const float * samples, int count,
                                        const char * language, int threads, int beam,
                                        char * text, size_t text_size, char * error, size_t error_size) {
#ifdef DIPPER_HAS_WHISPER
    try {
        auto * context = static_cast<DipperContext *>(opaque);
        auto parameters = whisper_full_default_params(WHISPER_SAMPLING_BEAM_SEARCH);
        parameters.language = language;
        parameters.detect_language = false;
        parameters.translate = false;
        parameters.n_threads = threads;
        parameters.beam_search.beam_size = beam;
        parameters.no_context = true;
        parameters.initial_prompt = nullptr;
        parameters.vad = false;
        parameters.print_special = false;
        parameters.print_progress = false;
        parameters.print_realtime = false;
        parameters.print_timestamps = false;
        int status = whisper_full(context->native, parameters, samples, count);
        if (status != 0) throw std::runtime_error("Whisper inference failed with code " + std::to_string(status));
        std::string transcript;
        for (int index = 0; index < whisper_full_n_segments(context->native); ++index) {
            const char * segment = whisper_full_get_segment_text(context->native, index);
            if (!segment) throw std::runtime_error("Whisper returned a null segment");
            size_t length = std::strlen(segment);
            if (length >= text_size || transcript.size() >= text_size - length)
                throw std::runtime_error("raw transcript exceeds configured limit");
            transcript.append(segment, length);
        }
        std::memcpy(text, transcript.c_str(), transcript.size() + 1);
        return 0;
    } catch (const std::exception & exception) { copy_text(error, error_size, exception.what()); }
    catch (...) { copy_text(error, error_size, "native inference failed with unknown exception"); }
#else
    (void)opaque; (void)samples; (void)count; (void)language; (void)threads; (void)beam; (void)text; (void)text_size;
    copy_text(error, error_size, "native runtime unavailable");
#endif
    return -1;
}

extern "C" void dipper_whisper_free(void * opaque) {
#ifdef DIPPER_HAS_WHISPER
    auto * context = static_cast<DipperContext *>(opaque);
    if (context) {
        if (context->native) whisper_free(context->native);
        whisper_log_set(nullptr, nullptr);
        ggml_log_set(nullptr, nullptr);
        delete context;
    }
#else
    (void)opaque;
#endif
}
