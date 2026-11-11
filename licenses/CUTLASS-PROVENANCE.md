# Cutlass — provenance scan

Ran 2026-10-29 against `crates/cutlass/` (identical bytes to
`references/cutlass/` at commit `9580057689e47e9adc4933c509fa074597ad5280`).
Companion to `CUTLASS-NOTICE`; this covers the "was the licensor entitled to license
it" question that the licence text itself cannot answer.

## Bottom line

No evidence of copied copyleft code. What is documented here is an *absence of signal*
from a method that has a known blindness, so the residual risk is not zero — see
Limits. Nothing found requires pulling a crate out of the tree.

## What the upstream records themselves say

Five codec crates carry `STATUS.md`, and `containers/mp3` carries `SUPPORT.md`. They
name the sources that were consulted before implementation, in file-level detail:

> Read before implementation: FFmpeg 8.1.1 `libavformat/mp3dec.c`, `mp3enc.c`,
> `id3v2.c`, `id3v1.c`, and `libavcodec/mpegaudiodecheader.c` with `mpegaudiotabs.h`;
> GStreamer `gstmpegaudioparse.c`; LAME `VbrTag.c`; mpg123 `parse.c`; the Symphonia MP3
> demuxer. **Those trees were not copied into this repo.** — `containers/mp3/SUPPORT.md`

> dav1d 1.5.x (`obu.c`, `msac.c`, `decode.c`, `recon_tmpl.c`, `ipred_tmpl.c`,
> `itx_1d.c`, `lf_apply_tmpl.c`) is the production decoder that was read. … rav1e
> (`encoder.rs`, `ec.rs`, `predict.rs`) is the production encoder that was read. …
> **This crate does not copy those sources.** — `codecs/av1/STATUS.md`

FFmpeg is used as a *behavioural* oracle, not a dependency:
`containers/isobmff/src/oracle.rs` opens with "FFmpeg and GStreamer are oracles, not
dependencies", and AV1 tests assert `quantizer_matrix_matches_dav1d` and "the pictures
match dav1d". Reading a GPL implementation before writing your own is not a clean room,
so the attestation is worth something precisely because it is testable. It is also
self-asserted, which is why the scan below exists.

## History forensics

| | |
|---|---|
| Commit dates | 108 on 2026-10-28, 164 on 2026-10-29 — the entire project is two days old |
| Largest single-author commits | 16,035 lines "Decode and encode progressive H.264…"; 9,617 lines AV1; 5,382 lines AAC-LC; 6,073 lines ISOBMFF |
| Commits by `Cursor Agent` | 12, including 3,673 lines "Add VP9 tables, the bool coder, and inverse transforms" |
| Branch naming | `cursor/vp9-encoder-decoder-d4af` — agent-authored PR branches |
| Copyright or SPDX headers in 293 `.rs` files | none |
| Test coverage | ~25,460 of 116,329 lines (21%) in `#[cfg(test)]` modules; 30 crates self-test |

A 16k-line spec-correct CABAC encoder does not arrive in one commit by hand. The tree is
model-generated. That is the provenance *risk*, but it is not by itself evidence of
copying — a model can write a codec from a specification.

## Similarity scan

Corpora fetched and compared (≈35k lines of C, ≈166 Rust files):

- FFmpeg `n7.1`: `h264_cavlc.c`, `h264_cabac.c`, `h264_ps.c`, `h264dec.c`, `mp3dec.c`,
  `mpegaudiodecheader.c`, `mpegaudiotabs.c`, `id3v2.c`, `mov.c`, `matroskadec.c`,
  `mpegts.c`, `flacdec.c`, `vorbisdec.c`, `opus/celt.c`, `proresdec.c`
- x264 `stable` (GPL): `encoder/cabac.c`, `encoder/analyse.c`, `encoder/rdo.c`,
  `encoder/macroblock.c`, `common/mc.c`, `common/quant.c`
- Symphonia 0.6.1 (MPL-2.0), all 166 source files, from the local registry
- rav1e `src/ec.rs` and dav1d `src/msac.c`, `src/obu.c` — the two files the AV1 crate
  names as read

Two metrics over all 231 copied files. **Verbatim**: comment-stripped token n-grams of
10–16 tokens. **Skeleton**: the same with every identifier and every numeric literal
abstracted to a single class, so it survives renaming — but also loses all content.

### Results

Verbatim, whole tree vs the FFmpeg+x264 corpus at n=16: 15 of 231 files registered any
hit. Every hit inspected was identifier-free — runs of zeros, ascending integer sequences,
and comma padding inside numeric literal tables (`codecs/h264/src/tables.rs` 115 hits
against `h264_cavlc.c`; `codecs/vp9/src/tables.rs` the same shape). Those are the ISO/ITU-T
and AOM normative tables every conformant decoder reproduces, not implementation data. The
other recurring hit class was `#[cfg(test)] mod tests { use super::` boilerplate.

The most sensitive test available is Rust-against-Rust: the AV1 crate's 21 files vs
rav1e's `ec.rs`, the exact file its `STATUS.md` admits to reading. **20 of 21 files share
zero 14-grams; `restore.rs` shares two.** Verbatim copying by a model is precisely what a
14-gram test catches, and it is absent.

Skeleton results are uninformative and must not be read as a signal. Calibrated controls:
two *independent* implementations of the same H.264 spec (ffmpeg `h264_cavlc.c` vs x264
`cabac.c`) score 94 shared 16-grams and 68 shared 20-grams — higher than any
Cutlass-to-GPL pair (worst: `enc.rs` vs `x264_quant.c` at 27, and those 27 are all
variants of `acc = a[i] + b[j]; acc = a[i] - b[j];`, the universal butterfly idiom).
Within Rust, cutlass `symbol.rs` vs cutlass `tile.rs` — same crate, same author, no
copying possible — scores 113 shared skeleton-18-grams, against 134 for
cutlass `tile.rs` vs rav1e `ec.rs`. The metric's floor swamps its signal.

One validated method limit, stated plainly: the verbatim test does not fire on a window
whose identifiers were all renamed, so it detects paste-shaped copying and not
structure-preserving translation. The skeleton test would cover the latter but saturates.

## Open item — one data blob

`codecs/av1/src/quantizer_matrix.bin` is 100,320 bytes, pulled into the crate with
`include_bytes!` as a `static MATRIX`, so it **ships inside the Halo binary**. Code
similarity testing says nothing about the provenance of an opaque blob. AV1 quantiser
matrices are normative in the AOMedia specification (CC-BY 4.0, attribution required), and
the crate's own test verifies the matrix "matches dav1d". Either is fine, but the
provenance needs one answer from the author: generated from the spec formula, or dumped
from dav1d/libaom? If dumped, dav1d's BSD-3 notice has to be retained and added to
`licenses/`. The `codecs/vp9/fixtures/*.bin|*.yuv` files are also `include_bytes!`, but all
of them sit under `#[cfg(test)]` and are not in any shipped artifact.

## Limits of this scan

The reference corpus is a slice, not the whole: FFmpeg's codec surface is far larger than
the 15 files used, and dav1d (only 2 of the 7 files named in `STATUS.md`), libvpx, libaom,
x265, GStreamer, LAME and mpg123 were largely not compared. A model that paraphrases an
algorithm it has memorised, rather than reproducing it, is undetectable by n-gram methods
at any n. So this scan can reasonably say "no copied code found"; it cannot say "the
code is demonstrably clean". The residual risk concentrates exactly where the patent-pool
risk already sits: `h264`, `hevc`, `av1`, `vp9`, `mpeg2`, `mpeg4`, `aac`, `mp3`, `prores`.

## Recommended next actions, in order

1. Ask the author two questions in writing: how `quantizer_matrix.bin` was produced, and
   which model/toolchain generated the codec crates. Cheap, and it settles the one
   unanswerable item.
2. Before a release that ships H.264/HEVC/AC-3/MPEG-2 encode or decode, clear the patent
   pool question with counsel — the licence was never going to cover it.
3. If Halo grows dependent on these codecs, run a real similarity audit
   (ScanCode/ORT, or Fossology with a full FFmpeg + dav1d + libvpx corpus) rather than
   this n-gram approximation, and keep the output next to this file.
4. Add `cargo about generate` to the release checklist so the shipped notices cover all
   1,181 resolved packages rather than the handful listed in the NOTICE.
5. Keep the pristine upstream clone at `references/cutlass/` until the tree is either
   adopted or deleted — it is the only record of what was licensed at which commit.
