/*
 * Shim C minimal au-dessus de whisper.cpp.
 *
 * Raison d'être : `whisper_full_params` et `whisper_context_params` sont de
 * gros structs C (pointeurs de fonctions imbriqués, sous-structs, ordre et
 * alignement dépendants de l'implémentation MSVC). Les décrire *à la main*
 * en Rust est source d'erreurs silencieuses. On les remplit ici, côté C, et
 * on n'expose à Rust qu'une API plate de fonctions simples (entiers,
 * pointeurs, tableaux).
 *
 * Ce fichier est compilé en C (l'API whisper.h est compatible C) et lié
 * avec les libs statiques `whisper` + `ggml` produites par CMake
 * (voir build.rs).
 *
 * Contrat d'audio : PCM 16-bit signé, 16 kHz, mono (WAV produit par
 * media::to_wav_16k). La conversion s16 → f32 est faite ici, une fois,
 * au moment de l'appel.
 */

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "whisper.h"

/* Poignée opaque : un contexte whisper + options. */
typedef struct {
    struct whisper_context *ctx;
    int n_threads;
} TldlWhisper;

/*
 * Progression (0..100), globale au processus : une seule transcription à la
 * fois par construction (ASR/LLM jamais concurrents).
 */
static int g_progress = 0;

static void tldl_progress_cb(struct whisper_context *ctx,
                             struct whisper_state *state,
                             int progress,
                             void *user_data) {
    (void)ctx;
    (void)state;
    (void)user_data;
    if (progress < 0) progress = 0;
    if (progress > 100) progress = 100;
    g_progress = progress;
}

/*
 * Initialise un contexte whisper.
 * Retourne NULL si le modèle n'a pas pu être chargé.
 * use_gpu : 0 = CPU, 1 = GPU (CUDA — sans effet sur une build CPU-only).
 */
TldlWhisper *tldl_whisper_init(const char *model_path, int use_gpu, int n_threads) {
    struct whisper_context_params cp = whisper_context_default_params();
    cp.use_gpu = (use_gpu != 0);

    TldlWhisper *w = (TldlWhisper *)calloc(1, sizeof(TldlWhisper));
    if (!w) return NULL;
    w->ctx = whisper_init_from_file_with_params(model_path, cp);
    if (!w->ctx) {
        free(w);
        return NULL;
    }
    /* 0 (ou moins) = « pas de préférence » : on garde la valeur par défaut
     * de whisper_full_default_params (min(4, nb de cœurs)). */
    w->n_threads = n_threads;
    return w;
}

void tldl_whisper_free(TldlWhisper *w) {
    if (!w) return;
    if (w->ctx) whisper_free(w->ctx);
    free(w);
}

/* Progression de la transcription en cours (0..100), -1 si aucune. */
int tldl_whisper_get_progress(void) {
    return g_progress;
}

/*
 * Transcrit `samples` (PCM s16, 16 kHz, mono) de `n_samples` échantillons.
 * `language` : "auto" (détection) ou un code ISO ("fr", "en", ...).
 * Retourne le code de `whisper_full` (0 = ok, -1 = erreur d'usage).
 */
int tldl_whisper_transcribe_s16(TldlWhisper *w,
                                const int16_t *samples,
                                int n_samples,
                                const char *language) {
    if (!w || !w->ctx || !samples || n_samples <= 0) return -1;

    struct whisper_full_params wp = whisper_full_default_params(WHISPER_SAMPLING_GREEDY);
    if (w->n_threads > 0) {
        wp.n_threads = w->n_threads;
    }
    /*
     * `detect_language=true` fait « détecter puis retourner » (aucune
     * transcription) dans v1.9.5 : on laisse donc false et on passe
     * "auto" comme langue — whisper_full détecte la langue ET transcrit.
     */
    wp.language = (language != NULL && strcmp(language, "auto") != 0) ? language : (const char *)"auto";
    wp.suppress_blank = true;
    wp.print_progress = false;
    wp.print_realtime = false;
    wp.progress_callback = tldl_progress_cb;
    wp.progress_callback_user_data = (void *)0;

    /* whisper_full() ne prend que du f32 : conversion locale, libérée après. */
    float *f = (float *)malloc((size_t)n_samples * sizeof(float));
    if (!f) return -1;
    for (int i = 0; i < n_samples; ++i) {
        f[i] = (float)samples[i] / 32768.0f;
    }

    g_progress = 0;
    const int rc = whisper_full(w->ctx, wp, f, n_samples);
    free(f);
    return rc;
}

int tldl_whisper_n_segments(TldlWhisper *w) {
    if (!w || !w->ctx) return -1;
    return whisper_full_n_segments(w->ctx);
}

/* Horodatages d'un segment, en centièmes de seconde. */
void tldl_whisper_get_segment_time(TldlWhisper *w, int i,
                                   int64_t *t0_cs, int64_t *t1_cs) {
    if (!w || !w->ctx) {
        if (t0_cs) *t0_cs = 0;
        if (t1_cs) *t1_cs = 0;
        return;
    }
    if (t0_cs) *t0_cs = whisper_full_get_segment_t0(w->ctx, i);
    if (t1_cs) *t1_cs = whisper_full_get_segment_t1(w->ctx, i);
}

/*
 * Copie le texte du segment `i` dans `buf` (terminé par '\0').
 * Retourne la longueur copiée (hors '\0'), ou -1 si erreur.
 */
int tldl_whisper_get_segment_text(TldlWhisper *w, int i, char *buf, int buf_len) {
    if (!w || !w->ctx || !buf || buf_len <= 0) return -1;
    const char *t = whisper_full_get_segment_text(w->ctx, i);
    if (!t) return -1;
    int len = (int)strlen(t);
    if (len > buf_len - 1) len = buf_len - 1;
    memcpy(buf, t, (size_t)len);
    buf[len] = '\0';
    return len;
}

/*
 * Langue détectée (code ISO, "fr", "en", ...) copiée dans `buf`.
 * Retourne la longueur copiée, ou -1 si inconnue.
 */
int tldl_whisper_lang_str(TldlWhisper *w, char *buf, int buf_len) {
    if (!w || !w->ctx || !buf || buf_len <= 0) return -1;
    const char *s = whisper_lang_str(whisper_full_lang_id(w->ctx));
    if (!s) return -1;
    int len = (int)strlen(s);
    if (len > buf_len - 1) len = buf_len - 1;
    memcpy(buf, s, (size_t)len);
    buf[len] = '\0';
    return len;
}
