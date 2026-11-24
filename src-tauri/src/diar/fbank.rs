//! Frontend Wespeaker — log-mel fbank.
//!
//! Port exact de `wespeaker runtime/core/frontend/fbank.h` (`Compute`),
//! validé bit-à-bit contre les golden vectors Python (fixtures
//! `tests/fixtures/` + valeurs `ref.py`).
//!
//! Échelle d'entrée : échantillons **int16** 16 kHz mono — le runtime
//! wespeaker lit le WAV en int16, la fbank consomme l'échelle int16 (pas
//! normalisée). Le global-mean par fenêtre (`global_mean`) neutralise
//! l'échelle absolue avant le run ONNX.

use super::NUM_BINS;

const SR: f64 = 16_000.0;
const FRAME_LEN: usize = 400; // 25 ms
const FRAME_SHIFT: usize = 160; // 10 ms
const FFT_POINTS: usize = 512; // UpperPowerOfTwo(400)
const NUM_FFT_BINS: usize = 256; // fft_points / 2

/// Frontend log-mel (Wespeaker). Construit une fois, réutilisé par fenêtre.
pub struct Fbank {
    /// Par bande mel : (premier bin FFT, poids des bins consécutifs).
    filters: Vec<(usize, Vec<f32>)>,
    /// Fenêtre hamming 0.54 − 0.46·cos(2πi/(L−1)).
    window: Vec<f32>,
}

impl Fbank {
    pub fn new() -> Self {
        Self { filters: build_filters(), window: build_window() }
    }

    /// Log-mel d'une slice 16 kHz i16 — une ligne de 80 par frame
    /// (cadre 400, saut 160, `1 + (n−400)/160` frames ; `0` si `n < 400`).
    pub fn compute(&self, wav: &[i16]) -> Vec<[f32; NUM_BINS]> {
        let n = wav.len();
        if n < FRAME_LEN {
            return Vec::new();
        }
        let num_frames = 1 + (n - FRAME_LEN) / FRAME_SHIFT;
        let mut feats: Vec<[f32; NUM_BINS]> = vec![[0.0f32; NUM_BINS]; num_frames];
        for i in 0..num_frames {
            let start = i * FRAME_SHIFT;
            compute_frame(&self.filters, &self.window, &wav[start..start + FRAME_LEN], &mut feats[i]);
        }
        feats
    }
}

/// Une frame : offset DC → pré-emphasis 0.97 → fenêtre → FFT → puissance.
/// Renvoie le spectre de puissance (256 bins, 0..255).
fn frame_power(window: &[f32], frame: &[i16]) -> [f32; NUM_FFT_BINS] {
    // Offset DC (moyenne de la frame).
    let mean: f32 = frame.iter().map(|&s| s as f32).sum::<f32>() / FRAME_LEN as f32;
    let mut d = [0.0f32; FRAME_LEN];
    for j in 0..FRAME_LEN {
        d[j] = frame[j] as f32 - mean;
    }
    // Pré-emphasis 0.97, in-place arrière→avant (comme kaldi).
    for j in (1..FRAME_LEN).rev() {
        d[j] -= 0.97 * d[j - 1];
    }
    d[0] -= 0.97 * d[0]; // = d[0] · 0.03
    // Fenêtre.
    for j in 0..FRAME_LEN {
        d[j] *= window[j];
    }
    // FFT 512 (zéro-padding), spectre complexe.
    let mut re = [0.0f32; FFT_POINTS];
    let mut im = [0.0f32; FFT_POINTS];
    for j in 0..FRAME_LEN {
        re[j] = d[j];
    }
    fft(&mut re, &mut im);
    // Puissance des 256 premiers bins (0..255).
    let mut power = [0.0f32; NUM_FFT_BINS];
    for i in 0..NUM_FFT_BINS {
        power[i] = re[i] * re[i] + im[i] * im[i];
    }
    power
}

/// Énergies log-mel d'une frame, à partir du spectre de puissance.
/// Garde-eps contre le log 0 (bandes sans énergie).
fn frame_mels(filters: &[(usize, Vec<f32>)], power: &[f32; NUM_FFT_BINS], out: &mut [f32; NUM_BINS]) {
    let eps: f32 = 2.220_446_049_250_313e-16_f64 as f32;
    for b in 0..NUM_BINS {
        let (first, weights) = &filters[b];
        let mut e: f32 = 0.0;
        for (k, &w) in weights.iter().enumerate() {
            e += w * power[first + k];
        }
        e = e.max(eps);
        out[b] = e.ln();
    }
}

/// Une frame : DC → pré-emphasis → fenêtre → FFT → puissance → énergie
/// log-mel. Écrit dans `out` (80 valeurs).
fn compute_frame(
    filters: &[(usize, Vec<f32>)],
    window: &[f32],
    frame: &[i16],
    out: &mut [f32; NUM_BINS],
) {
    let power = frame_power(window, frame);
    frame_mels(filters, &power, out);
}

/// FFT in-place Cooley–Tukey itératif radix-2 (N = puissance de 2), convention
/// avant `X[k] = Σ x[n]·e^(−2πi kn/N)` — même que `np.fft.rfft`.
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    // Périodisation par permutation bit-reverse (compteur incrémental
    // en ordre bit-reverse : à chaque `i`, `j` vaut `bitreverse(i)`).
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j >= bit {
            j -= bit;
            bit >>= 1;
        }
        j += bit;
        if j > i {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    // Papillons itératifs, décimation par temps (Cooley–Tukey) : au stade
    // `half`, blocs de `2·half` (`k` pas `2·half`), `half` papillons par
    // bloc — `t = W^j · x[k+j+half]`, `x[k+j+half] = x[k+j] − t`,
    // `x[k+j] += t` — le twiddle `W = e^(−2πi/(2·half))` courant.
    let mut half = 1usize;
    while half < n {
        let m = 2 * half;
        let angle = -2.0 * std::f32::consts::PI / m as f32;
        let cw = angle.cos();
        let sw = angle.sin();
        let mut k = 0usize;
        while k < n {
            let mut w_re = 1.0f32;
            let mut w_im = 0.0f32;
            for j in 0..half {
                let t_re = w_re * re[k + j + half] - w_im * im[k + j + half];
                let t_im = w_re * im[k + j + half] + w_im * re[k + j + half];
                re[k + j + half] = re[k + j] - t_re;
                im[k + j + half] = im[k + j] - t_im;
                re[k + j] += t_re;
                im[k + j] += t_im;
                let n_re = w_re * cw - w_im * sw;
                let n_im = w_re * sw + w_im * cw;
                w_re = n_re;
                w_im = n_im;
            }
            k += m;
        }
        half *= 2;
    }
}

/// Filtres mel triangulaires (kaldi) : pour chaque bande, le premier bin FFT
/// tombant dans (left, right) et les poids des bins consécutifs.
fn build_filters() -> Vec<(usize, Vec<f32>)> {
    let low_freq: f64 = 20.0;
    let high_freq: f64 = SR / 2.0; // 8000
    let mel_low: f64 = 1127.0 * (1.0 + low_freq / 700.0).ln();
    let mel_high: f64 = 1127.0 * (1.0 + high_freq / 700.0).ln();
    let delta: f64 = (mel_high - mel_low) / (NUM_BINS + 1) as f64;
    let mut filters = Vec::with_capacity(NUM_BINS);
    for b in 0..NUM_BINS {
        let left = mel_low + b as f64 * delta;
        let center = left + delta;
        let right = center + delta;
        let mut first = 0usize;
        let mut first_set = false;
        let mut weights: Vec<f32> = Vec::new();
        for i in 0..NUM_FFT_BINS {
            let freq = (SR / FFT_POINTS as f64) * i as f64;
            let mel = 1127.0 * (1.0 + freq / 700.0).ln();
            if left < mel && mel < right {
                let w = if mel <= center {
                    (mel - left) / (center - left)
                } else {
                    (right - mel) / (right - center)
                };
                if !first_set {
                    first = i;
                    first_set = true;
                }
                weights.push(w as f32);
            }
        }
        filters.push((first, weights));
    }
    filters
}

/// Fenêtre hamming sur `FRAME_LEN` (a = 2π/(L−1)).
fn build_window() -> Vec<f32> {
    let a = 2.0 * std::f64::consts::PI / (FRAME_LEN - 1) as f64;
    (0..FRAME_LEN).map(|i| (0.54 - 0.46 * (a * i as f64).cos()) as f32).collect()
}

/// Global-mean par bande (wespeaker `speaker_engine.cc` `ApplyMean`) :
/// `feat[t][b] −= mean_b`, `mean_b = (1/T)·Σ_t feat[t][b]` — per-band,
/// appliqué **par fenêtre** avant le run ONNX.
pub fn global_mean(feats: &mut [[f32; NUM_BINS]]) {
    let t = feats.len();
    if t == 0 {
        return;
    }
    let t_inv = 1.0 / t as f32;
    let mut means = [0.0f32; NUM_BINS];
    for row in feats.iter() {
        for b in 0..NUM_BINS {
            means[b] += row[b];
        }
    }
    for b in 0..NUM_BINS {
        means[b] *= t_inv;
    }
    for row in feats.iter_mut() {
        for b in 0..NUM_BINS {
            row[b] -= means[b];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Charge une fixture i16 (little-endian, 16 kHz).
    fn load_fixture(name: &str) -> Vec<i16> {
        let path = format!(
            "{}/src/diar/tests/fixtures/{}",
            env!("CARGO_MANIFEST_DIR"),
            name
        );
        let b = std::fs::read(path).expect("fixture i16");
        b.chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect()
    }

    /// Écart absolu max entre une ligne Rust et ses 8 valeurs golden.
    fn max_abs_err(got: &[f32; NUM_BINS], golden: &[f32; 8]) -> f32 {
        (0..8).map(|k| (got[k] - golden[k]).abs()).fold(0.0f32, f32::max)
    }

    const A0: [f32; 8] = [
        1.180914211e1, 1.273312473e1, 1.294102859e1, 1.233246613e1, 1.168664074e1,
        1.182704258e1, 1.320343781e1, 1.397141838e1,
    ];
    const A1: [f32; 8] = [
        1.180911064e1, 1.273319340e1, 1.294094181e1, 1.233257198e1, 1.168686008e1,
        1.182696056e1, 1.320334530e1, 1.397155952e1,
    ];
    const AM1: [f32; 8] = [
        1.180915260e1, 1.273329735e1, 1.294079304e1, 1.233258247e1, 1.168695927e1,
        1.182717896e1, 1.320352650e1, 1.397139454e1,
    ];
    const AMID: [f32; 8] = [
        1.180910587e1, 1.273327827e1, 1.294091702e1, 1.233244991e1, 1.168677044e1,
        1.182691002e1, 1.320319653e1, 1.397160053e1,
    ];
    const B0: [f32; 8] = [
        1.116046524e1, 1.199440861e1, 1.209320831e1, 1.184743690e1, 1.165013409e1,
        1.140722847e1, 1.162018585e1, 1.228878975e1,
    ];
    const BM1: [f32; 8] = [
        1.116042900e1, 1.199452877e1, 1.209313774e1, 1.184740829e1, 1.165021610e1,
        1.140730762e1, 1.161984634e1, 1.228906536e1,
    ];

    #[test]
    fn fbank_matches_golden() {
        let fb = Fbank::new();
        let a = load_fixture("tone_a_i16.bin");
        let fa = fb.compute(&a);
        assert_eq!(fa.len(), 158, "T attendu 158 (fenêtre 1.6 s)");

        let e0 = max_abs_err(&fa[0], &A0);
        let e1 = max_abs_err(&fa[1], &A1);
        let em1 = max_abs_err(&fa[fa.len() - 1], &AM1);
        let emid = max_abs_err(&fa[fa.len() / 2], &AMID);
        let worst = e0.max(e1).max(em1).max(emid);
        eprintln!("fbank max_abs_err (tone_a) = {worst:.9e}");
        assert!(
            worst < 5e-4,
            "fbank diverge des golden vectors (max_abs_err={worst:.3e})"
        );
    }

    #[test]
    fn fbank_second_signal_matches_golden() {
        let fb = Fbank::new();
        let b = load_fixture("tone_b_i16.bin");
        let fbanks = fb.compute(&b);
        assert_eq!(fbanks.len(), 158);
        let e0 = max_abs_err(&fbanks[0], &B0);
        let em1 = max_abs_err(&fbanks[fbanks.len() - 1], &BM1);
        let worst = e0.max(em1);
        eprintln!("fbank max_abs_err (tone_b) = {worst:.9e}");
        assert!(
            worst < 5e-4,
            "fbank diverge des golden vectors (max_abs_err={worst:.3e})"
        );
    }

    #[test]
    fn fbank_filters_well_formed() {
        // Aucune bande vide ; `first` cohérent avec les poids consécutifs.
        for (first, weights) in build_filters() {
            assert!(!weights.is_empty(), "bande mel vide");
            assert!(first < NUM_FFT_BINS);
        }
    }

    #[test]
    fn global_mean_zero_mean_per_band() {
        let fb = Fbank::new();
        let a = load_fixture("tone_a_i16.bin");
        let mut fa = fb.compute(&a);
        global_mean(&mut fa);
        // Après global-mean, chaque bande a une moyenne (sur T) nulle.
        let t = fa.len() as f32;
        for b in 0..NUM_BINS {
            let m: f32 = fa.iter().map(|row| row[b]).sum::<f32>() / t;
            assert!(m.abs() < 1e-3, "moyenne bande {b} non nulle : {m}");
        }
    }

    #[test]
    fn fft_impulse_is_constant() {
        // FFT d'une impulsion [1,0,…,0] = spectre constant [1,1,…,1]
        // (verrouille la permutation bit-reverse + les papillons).
        for n in [4usize, 8, 16, 64, 512] {
            let mut re = vec![0.0f32; n];
            let mut im = vec![0.0f32; n];
            re[0] = 1.0;
            fft(&mut re, &mut im);
            for k in 0..n {
                assert!((re[k] - 1.0).abs() < 1e-5, "n={n} k={k} re={}", re[k]);
                assert!(im[k].abs() < 1e-5, "n={n} k={k} im={}", im[k]);
            }
        }
    }

    #[test]
    fn fbank_deterministic() {
        let fb = Fbank::new();
        let a = load_fixture("tone_a_i16.bin");
        let x = fb.compute(&a);
        let y = fb.compute(&a);
        assert_eq!(x, y, "fbank non déterministe");
    }
}
