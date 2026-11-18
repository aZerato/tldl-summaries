/* tldl_llama_shim.c — API C plate sur llama.cpp (b10884), pour Rust.
 *
 * Miroir du shim whisper : un moteur opaque (modèle + contexte), un chargement
 * du GGUF, et une génération de texte depuis un prompt UTF-8.
 *
 * GPU optionnel (C3, F10) : `n_gpu_layers` est passé par l'appelant
 * (-1 = toutes les couches sur le GPU, 0 = CPU) ; sans effet si la build
 * n'active aucun backend GPU (cf. build.rs / `TLDL_GPU`).
 */

#include "llama.h"

#include <stdlib.h>
#include <string.h>

typedef struct {
    struct llama_model * model;
    struct llama_context * ctx;
    const struct llama_vocab * vocab;
    int32_t n_ctx;
} TldlLlama;

/* Callback de progression : (tokens produits, max) ; user = pointeur opaque. */
typedef int32_t (*TldlLlamaProgress)(int32_t produced, int32_t max, void * user);

static int g_backend_init = 0;

/* Charge un GGUF dans `*out`. `n_gpu_layers` : -1 = toutes les couches sur
 * le GPU, 0 = CPU (F10). Retourne 0, ou -1 avec `err` remplie. */
int tldl_llama_load(const char * path, int32_t n_ctx, int32_t n_gpu_layers,
                    TldlLlama ** out, char * err, int32_t err_len) {
    if (err != NULL) {
        err[0] = '\0';
    }
    if (!g_backend_init) {
        llama_backend_init();
        g_backend_init = 1;
    }

    struct llama_model_params mp = llama_model_default_params();
    mp.n_gpu_layers = n_gpu_layers; /* F10 : -1 = GPU (tout), 0 = CPU */
    struct llama_model * model = llama_model_load_from_file(path, mp);
    if (model == NULL) {
        if (err != NULL) {
            snprintf(err, (size_t)err_len, "chargement du modèle échoué : %s", path);
        }
        return -1;
    }

    struct llama_context_params cp = llama_context_default_params();
    cp.n_ctx = (uint32_t)n_ctx;
    cp.n_batch = 512;
    struct llama_context * ctx = llama_init_from_model(model, cp);
    if (ctx == NULL) {
        llama_model_free(model);
        if (err != NULL) {
            snprintf(err, (size_t)err_len, "initialisation du contexte échouée (n_ctx=%d)",
                     n_ctx);
        }
        return -1;
    }

    TldlLlama * e = (TldlLlama *)calloc(1, sizeof(TldlLlama));
    e->model = model;
    e->ctx = ctx;
    e->vocab = llama_model_get_vocab(model);
    e->n_ctx = n_ctx;
    *out = e;
    return 0;
}

void tldl_llama_free(TldlLlama * e) {
    if (e == NULL) {
        return;
    }
    if (e->ctx != NULL) {
        llama_free(e->ctx);
    }
    if (e->model != NULL) {
        llama_model_free(e->model);
    }
    free(e);
}

/* Génère depuis un prompt UTF-8 ; écrit le texte (sans nul) dans `out`.
 * Retourne le nombre d'octets écrits, ou -1 en cas d'erreur. */
int32_t tldl_llama_generate(TldlLlama * e, const char * prompt, int32_t prompt_len,
                           int32_t max_tokens, float temperature, float top_p,
                           uint32_t seed, char * out, int32_t out_max,
                           TldlLlamaProgress cb, void * user) {
    if (e == NULL || prompt == NULL || out == NULL) {
        return -1;
    }

    /* Tokenisation du prompt (BOS en tête si le modèle l'exige). */
    int32_t n_max = (prompt_len > 0 ? prompt_len : 1) + 16;
    llama_token * tokens = (llama_token *)malloc((size_t)n_max * sizeof(llama_token));
    if (tokens == NULL) {
        return -1;
    }
    int32_t n = llama_tokenize(e->vocab, prompt, prompt_len, tokens, n_max, false, true);
    if (n < 0) {
        free(tokens);
        return -1;
    }
    if (llama_vocab_get_add_bos(e->vocab)) {
        memmove(&tokens[1], &tokens[0], (size_t)n * sizeof(llama_token));
        tokens[0] = llama_vocab_bos(e->vocab);
        n += 1;
    }

    /* Batch : pré-alloué pour le prompt entier, réutilisé 1 token/itération. */
    struct llama_batch batch = llama_batch_init(n, 0, 1);
    batch.embd = NULL;
    batch.n_tokens = 0;
    for (int32_t i = 0; i < n; i++) {
        batch.token[batch.n_tokens] = tokens[i];
        batch.pos[batch.n_tokens] = (llama_pos)i;
        batch.n_seq_id[batch.n_tokens] = 1;
        batch.seq_id[batch.n_tokens][0] = 0;
        batch.logits[batch.n_tokens] = (i == n - 1) ? 1 : 0;
        batch.n_tokens++;
    }
    int32_t rc = llama_decode(e->ctx, batch);
    if (rc < 0) {
        llama_batch_free(batch);
        free(tokens);
        return -1;
    }

    /* Chaîne de sampling : temp -> top_p -> dist (graine fixe = reproductible). */
    struct llama_sampler * smpl =
        llama_sampler_chain_init(llama_sampler_chain_default_params());
    llama_sampler_chain_add(smpl, llama_sampler_init_temp(temperature));
    llama_sampler_chain_add(smpl, llama_sampler_init_top_p(top_p, 1));
    llama_sampler_chain_add(smpl, llama_sampler_init_dist(seed));

    int32_t n_out = 0;
    int32_t pos = n;
    for (int32_t k = 0; k < max_tokens; k++) {
        llama_token id = llama_sampler_sample(smpl, e->ctx, -1);
        if (llama_vocab_is_eog(e->vocab, id)) {
            break;
        }

        char piece[512];
        int32_t plen = llama_token_to_piece(e->vocab, id, piece, (int32_t)(sizeof piece) - 1,
                                            0, false);
        if (plen > 0) {
            int32_t take = (n_out + plen > out_max) ? (out_max - n_out) : plen;
            if (take > 0) {
                memcpy(out + n_out, piece, (size_t)take);
                n_out += take;
            }
            if (n_out >= out_max) {
                break;
            }
        }

        if (cb != NULL) {
            cb(k + 1, max_tokens, user);
        }

        batch.n_tokens = 1;
        batch.token[0] = id;
        batch.pos[0] = (llama_pos)pos++;
        batch.n_seq_id[0] = 1;
        batch.seq_id[0][0] = 0;
        batch.logits[0] = 1;
        rc = llama_decode(e->ctx, batch);
        if (rc != 0) {
            break;
        }
    }

    llama_sampler_free(smpl);
    llama_batch_free(batch);
    free(tokens);
    return n_out;
}
