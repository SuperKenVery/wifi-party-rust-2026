# Vocal Removal / Instrumental Extraction

Uses the **RT-DTT** real-time music source separation model to split a stereo mix into
four stems (bass, drums, other, vocals), then subtracts vocals from the original to
produce an instrumental track.

Reference implementation: `/Users/ken/Codes/tmp/dtt_infer/infer.py`
ONNX model: `/Users/ken/Codes/tmp/REALTIME_DTT/all_rt.onnx`

---

## 1. Audio Preprocessing

The model requires **stereo float32 PCM at 44 100 Hz**.
Any format ffmpeg understands can be converted:

```bash
ffmpeg -y -i input.m4a -ar 44100 -ac 2 -f wav input.wav
```

In Python this is done in `load_audio_44k()`, which writes to a temp WAV and reads it
back as a `(2, samples)` float32 numpy array via `soundfile`.

---

## 2. Chunking

The exported ONNX model accepts a **fixed input shape `(1, 4, 384, 64)`**, corresponding
to exactly one chunk of 32 256 samples (≈ 0.73 s at 44 100 Hz).

To process a full track the audio is sliced into overlapping windows:

| Parameter | Value | Notes |
|---|---|---|
| `INF_CHUNK` | 32 256 samples | `hop_length × (dim_t − 1)` = `512 × 63` |
| `OVERLAP` | 512 samples | `n_fft // 2` — padding on each side |
| `GEN_SIZE` | 31 232 samples | `INF_CHUNK − 2 × OVERLAP` — real content per chunk |

Padding scheme:

```
[ OVERLAP zeros | ... audio ... | right_pad zeros | OVERLAP zeros ]
                 └──── GEN_SIZE ────┘
```

Each window slides by `GEN_SIZE` so every chunk contains `OVERLAP` samples of
zero-padding on each side, which the STFT/iSTFT `center=True` mode needs to
reconstruct the edges cleanly.

---

## 3. STFT (waveform → spectrogram)

Implemented in `stft()` to match `AbstractModel_ALL.stft()` from the training code exactly.

```
input:  (1, 2, 32256)          – one chunk, stereo
reshape → (2, 32256)            – one row per channel
torch.stft(n_fft=1024, hop_length=512, window=hann(1024), center=True)
→ complex (2, 513, 64)
view_as_real → (2, 513, 64, 2)
permute → (2, 2, 513, 64)       – [ch, re/im, freq, time]
reshape → (1, 4, 513, 64)       – merge ch×re/im into channel dim
slice freq → (1, 4, 384, 64)    – keep only first 384 of 513 bins (DIM_F)
```

The 513 − 384 = 129 discarded high-frequency bins are padded back as zeros during
iSTFT.

---

## 4. ONNX Inference

```python
import onnxruntime as ort

sess = ort.InferenceSession("all_rt.onnx", providers=["CPUExecutionProvider"])
# Input:  (1, 4, 384, 64)  – spectrogram of one chunk
# Output: (1, 4, 4, 384, 64)  – per-source spectrograms
#                  ^
#                  4 sources: bass / drums / other / vocals (in that order)
output = sess.run(None, {"input": spec_in})[0]
```

CUDA is used automatically when `CUDAExecutionProvider` is available.

---

## 5. iSTFT (spectrogram → waveform)

Implemented in `istft()`, mirroring `AbstractModel_ALL.istft()`:

```
input:  (1, 4, 4, 384, 64)
pad freq back to 513 bins
reshape → (4, 2, 513, 64, 2)   – [source, ch, freq, time, re/im]
torch.complex + torch.istft(n_fft=1024, hop_length=512, center=True)
→ (1, 4, 2, 32256)              – [batch, source, ch, samples]
```

The overlap regions (`[:, :, :, :512]` and `[:, :, :, -512:]`) are then trimmed,
leaving `(1, 4, 2, 31232)` of clean content per chunk.

---

## 6. Reassembly

All chunks are concatenated along the time axis and trimmed to the original length:

```python
# out_chunks: list of (1, 4, 2, 31232)
all_chunks = np.concatenate(out_chunks, axis=0)   # (N, 4, 2, 31232)
result = all_chunks.transpose(1, 2, 0, 3)         # (4, 2, N, 31232)
             .reshape(4, 2, -1)                   # (4, 2, N×31232)
result = result[:, :, :orig_samples]              # trim to original length
```

---

## 7. Instrumental Output

The instrumental is the original mix minus the extracted vocal stem:

```python
instrumental = mix - separated[vocals_idx]   # (2, samples)
```

This is the standard "residual subtraction" approach: because the model's four stems
do **not** sum perfectly back to the mix, using the original mix as the base preserves
all non-vocal energy (including any artifacts the model might assign to the wrong stem).

---

## Running

```bash
cd /Users/ken/Codes/tmp/dtt_infer
uv run python infer.py <input_audio> /Users/ken/Codes/tmp/REALTIME_DTT/all_rt.onnx --outdir ./output
```

Outputs written to `--outdir`:

- `<stem>_bass.wav`
- `<stem>_drums.wav`
- `<stem>_other.wav`
- `<stem>_vocals.wav`
- `<stem>_instrumental.wav`  ← mix minus vocals
