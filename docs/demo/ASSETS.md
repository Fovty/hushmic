# Demo source assets — provenance & licensing

All assets below are **CC0, public domain, or permissive commercial-OK** (CC-BY).
No `-NC` or `-ND` assets were used. CC-BY assets are attributed here as required.

| Role | Local file | License | Commercial OK? | Attribution required? |
|------|-----------|---------|----------------|-----------------------|
| Voice | `assets/voice_c1.mp3` | Public Domain (LibriVox / PD Mark 1.0) | Yes | No (credited anyway) |
| Keyboard noise | `assets/keyboard_src.wav` | CC BY 4.0 | Yes | Yes — credited |
| Fan noise | `assets/fan_ac_src.wav` | CC BY 4.0 | Yes | Yes — credited |
| Café/office chatter | `assets/cafe_src.ogg` | Public Domain | Yes | No (credited anyway) |


---

## 1. Voice (clean speech)
- **Title:** Short Poetry Collection 266 — track 06, "Art and Heart" (public-domain poem)
- **Source item:** https://archive.org/details/spc266_2508_librivox
- **Direct file:** https://archive.org/download/spc266_2508_librivox/spc266_artheart_td_128kb.mp3
- **Author/credit:** Read by a LibriVox volunteer (catalog reader id `td`); text is a public-domain poem.
- **License:** Public Domain — LibriVox recordings are released into the public domain
  (item `licenseurl` = http://creativecommons.org/publicdomain/mark/1.0/).
- **Format used:** 44.1 kHz mono MP3 (128 kbps); a clean ~10 s passage from the poem body
  (offset 16.85 s, after the spoken LibriVox intro) was extracted, resampled to 48 kHz mono,
  and loudness-normalized to −20 dBFS RMS.

## 2. Keyboard noise (mechanical typing)
- **Title:** File:Typing on Keychron V1 Ultra (Red Linear Switch).wav
- **File page:** https://commons.wikimedia.org/wiki/File:Typing_on_Keychron_V1_Ultra_(Red_Linear_Switch).wav
- **Direct file:** https://upload.wikimedia.org/wikipedia/commons/2/27/Typing_on_Keychron_V1_Ultra_%28Red_Linear_Switch%29.wav
- **Author/credit:** Wikimedia Commons user **C40115** (own work).
- **License:** **CC BY 4.0** — https://creativecommons.org/licenses/by/4.0/
- **Format:** 48 kHz stereo WAV, 17.7 s. (Mechanical keyboard, linear switches.)

## 3. Fan noise (steady fan/AC hum)
- **Title:** File:Air conditioner hum (Gravity Sound).wav
- **File page:** https://commons.wikimedia.org/wiki/File:Air_conditioner_hum_(Gravity_Sound).wav
- **Direct file:** https://upload.wikimedia.org/wikipedia/commons/9/99/Air_conditioner_hum_%28Gravity_Sound%29.wav
- **Author/credit:** **Gravity Sound** (https://www.gravitysound.studio/).
- **License:** **CC BY 4.0** — https://creativecommons.org/licenses/by/4.0/
- **Format:** 44.1 kHz stereo WAV, 14 s. A steady broadband fan/AC hum (RMS ≈ −22 dBFS,
  very flat over time) — chosen as the closest faithful match to a continuous computer/PC fan hum,
  since no literal "PC fan" recording exists on Commons.

## 4. Café / office chatter
- **Title:** File:Restaurant ambience.ogg
- **File page:** https://commons.wikimedia.org/wiki/File:Restaurant_ambience.ogg
- **Direct file:** https://upload.wikimedia.org/wikipedia/commons/b/b5/Restaurant_ambience.ogg
- **Author/credit:** "stephan", originally via pdsounds.org.
- **License:** **Public Domain** (Commons file page: public domain; source pdsounds.org PD).
- **Format:** 44.1 kHz stereo OGG Vorbis, 76 s. Restaurant/café crowd chatter.

---

## Processing summary (how `before_*`/`after_*` were made)
1. Voice resampled to 48 kHz mono, normalized to −20 dBFS RMS.
2. Each noise trimmed to 10 s, resampled to 48 kHz mono, normalized to −23 dBFS RMS (→ **SNR ≈ +3 dB**).
3. `before_<noise>.wav` = voice + noise summed in float, then flat-gain peak-normalized to −1 dBFS
   (flat gain preserves the +3 dB SNR; verified no clipping).
4. `after_<noise>.wav` = `before` run through hushmic's offline enhancer
   (`crates/dpdfnet-ladspa/examples/enhance.rs`, DPDFNet-8 ONNX model).

Measured effect (gap = speech pause where only noise sits):
- Fan:  gap −23.4 → **−51.3 dBFS** (~28 dB suppressed); speech −15.1 → −16.0 dB (preserved).
- Café: gap −20.9 → **−48.0 dBFS** (~27 dB suppressed).
- Keyboard: keystroke transients removed; overall mean −22.2 → −24.0 dB.
