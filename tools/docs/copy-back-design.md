# Copy-back: card-to-card propagation (design note, no code)

Status 2026-11-26: design only. Facts: copy-back ~25-40 ms a frame (the thread has a ~40 ms floor of its own,
frame-budget.md item 5) for ~75-110 MB; the game reads back ~1.2 MB (27 rings, F1). run-50 readback line: "copy-back
this period: 8.0 MB inside the set, 3277.0 MB outside (99.8 % outside)" per 5 s. run-39 (`VICTORY_BACK_ONLY_READ=1`):
"copy-back skipped as never read: 2094.0 MB (... 1404.9 MB written later, when needed ...)": 67-71 % was needed later
by the LAYER, and frames got twice as slow because those bytes came back through settles and RAM refreshes.

## How a shader-written range travels today (shadow.rs, host.rs, readback.rs, inplace.rs)

1. Record time: `Shadow::gpu_writes` -> `HostList::defer_gpu_write` declares the range (snapshot `before`, or the CPU
   side of a card copy, is its reference). In-place copies declare nothing to copy back.
2. After the card ran: the card side is fetched (relay thread `VICTORY_RELAY`, wide pieces `VICTORY_WIDE_BACK`),
   compared with the reference by 64 KiB pieces (`differing` / `find_all`), merged with what the title wrote meanwhile
   (`merge_piece`), then `readback::back` (meter) and the write into title memory: through our own view
   (`put_back_own`, `VICTORY_BACK_OWN_VIEW=1`: journal told via `watch::wrote_own`, the producer's page noted current,
   every OTHER copy of the page takes it again) or the old way (guard opened, `write_mapped`).
3. Other copies learn it only from RAM: their next refresh (`Shadow::refresh` / `card_dirty` -> `Host::card_before`
   uploads; pages with many runs as a masked merge on the card, `VICTORY_MERGE_MASK`) reads title memory.
4. `VICTORY_BACK_ONLY_READ` held skipped bytes "card newer than title memory" in a ledger: `readback::overlay` patched
   every layer read of title memory (`read_into`, `copy_mapped`), `readback::settle` wrote them before a layer write,
   a remap (watch.rs) or a title lock. Those settles/overlays were the 67-71 %.

## Who consumes shader-written bytes

| Consumer | Why it exists | Bytes known? / meter |
|---|---|---|
| Game read-back rings (27, F1) | the title's CPU reads them | 8-16 MB per 5 s (~0.2-0.3 MB/frame): readback line "inside the set", "by ring" |
| Other card copies of the same title bytes: title-buffer copies (`CARD_BYTES`), windows (4 MiB grid over memory outside buffers, ~900, 3.5 GB, card sides by `VICTORY_CARD_WINDOWS_MB`), exact copies of single views (`shadow::exact`: views not on an element of their buffer's copy, ring positions in the big heap), tails/finer-grid duplicates | D3D12 needs a resource + aligned offset; console views address any GPU VA; one title byte can sit in a buffer copy, a window and an exact copy at once, each its own host buffer | NOT split. Total proxy: OWN_WRITES line ("stored through views of our own ... 86944 MB, 2016015 pages noted current for the copy that wrote them") |
| RAM-form copies (windows/buffers without a card side) | no room on the card / small | same: not split |
| Texture host copies sourced from buffers (placed textures over title memory; `descriptor::host::remapped`, texture refresh) | a buffer written by a dispatch, then sampled as a texture | not split; texture refresh counts uploads only |
| Hash / compare state: snapshots, card hashes (`CARD_BYTES[1]`), `heap_marks::back`, `terrain::landed` | finding what changed; meters | internal, follows the copies |
| Title CPU outside the rings (its own records in the shared heap: the "fence list read as -1" case) | engine data in the same heap | unproven; `VICTORY_READBACK_LOCKS=1` logs locks over shader-written memory outside the set |

The meter that would split it (stage 0, `VICTORY_BACK_CONSUMERS=1`): for every merged stretch in `copy_back_with`,
before writing, list the OTHER live copies overlapping it by kind (buffer card / buffer RAM / window card / window
RAM / exact / placed texture / other VA view of the same section page, from `inplace::write_own`'s `shown`), and
count bytes per kind; plus tag `readback::overlay`/`settle` callers by kind. Line: "copy-back consumers per frame:
rings R MB; card copies: buffer B, window W, exact E MB; RAM copies M MB; textures T MB; other views V MB; nobody N MB".
"nobody" is free to skip today; the card kinds are what this design moves.

## Proposal: propagate on the card, RAM only for the rings

After a producer flight P, for each declared range R of producer copy p and each other live CARD copy c overlapping R,
record `CopyBufferRegion(c.card, off_c, p.buffer, off_p, len)` in a propagation list put in the producer's queue right
behind P (same queue => ordered behind the producing list; one UAV barrier + COPY_SOURCE/DEST transitions per pair,
ranges joined per pair). c's pages are noted "current from the card at send N" (as `put_back_own` notes current), so
its refresh does not take them from RAM. The CPU still fetches and writes back ONLY: rings (the set), RAM-form
copies, ranges overlapping a consumer kind not yet moved, and whatever the lock meter shows the title reads.
Bytes not written to RAM go to the existing "card newer" ledger (`VICTORY_BACK_ONLY_READ` machinery) for overlay/settle.

Invariants:
- Queue order: propagation sits before P's own `Late::Signal` and before the next submission's `card_before`
  uploads; a consumer on another queue waits on the title's fence, which must be signalled after propagation (with
  `VICTORY_JOIN_PARTS`, after each joined part's propagation, never once at the end).
- Title rewrites of the same bytes on the CPU: c's next upload from RAM must not carry RAM's stale bytes over
  propagated ones. Pages in the ledger go up masked: mask = bytes where RAM differs from what it held when the write was
  skipped (the ledger already keeps that), uploaded with the masked merge (`card = card&!mask | side&mask`).
- Declared > written: the copy copies the whole declared range; outside the real writes p holds title bytes as of
  p's refresh. Safe only if c got no newer title bytes before the propagation executes: c's uploads for lists queued
  between P and the propagation must not exist (propagation is immediately behind P), and later uploads are masked.
- Unmap/remap: on `title_memory_unmapped` / remap of a range, settle its ledger bytes first (exists), drop the
  propagation targets under it; a window let go (unmap, idle, `VICTORY_WINDOWS_YIELD`) is re-made from RAM, so its
  ledger bytes must be settled or overlaid at that refresh (overlay already patches `read_into`).
- Views: the same physical page shown at two VAs (CPU view / GPU view, pool view; F3) is two "title ranges" to the
  layer. Consumers are found per section page (`inplace` views), not per VA, or the second view reads stale RAM.
- Texture sources: a placed texture over p's range is a consumer; stage 3 copies `CopyTextureRegion` from p.

## Staged plan
0. Meter `VICTORY_BACK_CONSUMERS` (above), one run. Go on only if card kinds are >= 50 % of the outside bytes.
1. One kind: windows with a card side overlapping a buffer card copy (`VICTORY_CARD_PROPAGATE=1`, windows only);
   RAM write skipped only where no other kind overlaps. Diag falsifier: sample 1 of 64 propagated stretches, fetch
   c's bytes after the next submission, compare with what the copy-back would have written; WARN "propagated bytes
   differ".
2. All card copies (buffer<->buffer, exact copies). 3. Textures sourced from buffers. 4. RAM writes = rings + ledger
   settles + lock-meter ranges only; fetch only what is written to RAM.

Expected saving: the outside bytes stop being fetched and written (~65-90 MB a frame of WC reads + RAM writes): most
of the 25-40 ms copy-back thread and its 40 ms floor; RAM refresh copies-in that follow copy-back (0.83 MB per MB in
runs 218-222 before own views) drop too. Card cost: ~90 MB a frame card-to-card (< 0.5 ms at card bandwidth) plus
barriers; risk of many small copies -> join per copy pair, cap pairs per flight, fall back to RAM past the cap.

## Risks by on-screen symptom (switch: `VICTORY_CARD_PROPAGATE=0`, back to today)
- Flicker, frozen or lagging animation, stale skinning/particles: a consumer missed (edge not propagated, other VA view).
- Garbage triangles / exploding meshes: declared-but-unwritten bytes of p overwrote newer title bytes (mask wrong).
- Wrong/black/blocky textures that fix themselves later: texture copies sourced from buffers not covered (stage 3).
- Hang or device removed: propagation after the fence signal, wrong resource state on c, copy on a closed list.
- Game-logic hiccups/hangs (fence list -1 class): title CPU reads heap bytes outside the rings -> keep the lock meter on.
- VRAM growth from more card copies kept alive: watch the housekeeping line.

## Built 2026-11-26 03:58-04:25 (consumers meter + skip-unread; not run yet)

The user's staged plan replaced the note's stage 1: the stages below are what was built. The note's propagation
proposal is now stage 2 (notes at the end).

- Stage 0, `VICTORY_BACK_CONSUMERS` (default 1 in diag builds): `victory-gpu/src/readback/consumers.rs`. Title memory in
  64 KiB pieces (title addresses); every copy-back write (`readback::back`) opens a generation of its piece (one per
  landing, `consumers::landing()` in `relay_land` and the two non-relay copy-back paths); the FIRST consumer after it takes
  all of the generation's bytes (no double count); written again or unread for 150 frames = "nothing". Consumers are seen
  at `host::read_into` / `host::copy_mapped` (source side), tagged by a thread-local kind: refreshes (`Shadow::update`,
  `merge_under_writes`: window / own-view (raw view's own bytes) / plain), texture host copies (`texture::with_read`),
  descriptor records, draw/indirect/wait records, layer hashing (`check_missed`, `refresh_check`), other layer reads;
  title locks (lock hook), title writes (guard fault, VEH), remaps, layer writes; the copy-back's own reads are ignored.
  Bytes inside the read-back set count as "title read-back" when written. Writer = shader hash of the last storing stage
  that declared writes on the copy (`set_writer` in `draw.rs`, `declared` in `Shadow::gpu_writes`).
  Line every 5 s: "copy-back consumers (...): copied back B MB in R ranges (C MB of copies in the card's memory); later
  consumed by: title read-back a MB, other card copies b MB (windows w, own-view o, plain p, texture sources t), layer
  hashing c MB, layer CPU reads d MB (descriptor records, draw/indirect records, other), title locks/writes, remaps and
  layer writes x MB, nothing e MB (f %); top 8 producer->consumer pairs by MB: [writer -> kind MB, ...]; pieces tracked N".
- Stage 1, `VICTORY_BACK_SKIP_UNREAD` (default 0; needs relay, `VICTORY_BACK_OWN_VIEW=1`, `VICTORY_INPLACE=0`):
  `consumers::try_skip` in `Host::relay_card_back` (copies in the card's memory, relay landing). A piece is left on the card
  (no fetch, no compare-merge, no write) when its last K=8 generations had no consumer, it is outside the read-back set at
  every address that shows it, every guard block over it stands (a title write faults first), our own view can write it,
  and the held total is under `VICTORY_BACK_SKIP_UNREAD_MB` (512). Kept: the copy (Arc, so its card side lives), offsets,
  title bytes at that moment (`t0`), the page at every view address. Other copies are told the page was written
  (`Shadow::note_skipped` = `put_back_own` without the write), so they re-read it and meet the piece. Demand
  (`consumers::demand`, one at a time): layer read/write of any of its addresses (hook in read_into/copy_mapped; explicit
  pre-demand in the merges that read under a reference lock), title write (VEH, before the write lands), title buffer lock
  (lock hook, installed with the switch; `VICTORY_READBACK_LOCKS=0` turns it off), remap/unmap; fetch =
  `host::demand_fetch` (own list on the direct queue behind everything sent, waited), merge =
  `Shadow::card_merge_relay_with` (as a landing; bytes where title memory no longer holds `t0`, or DirectStorage wrote
  since — `watch::title_memory_written` — stay the title's); journal/texture notes of a demand are queued and given at the
  next landing (`drain_notes`). A later landing that fetches the piece anyway merges it with `t0` and drops it. Demanded
  pieces are held to the old way for `VICTORY_BACK_SKIP_HOLD_FRAMES` (300). Line: "copy-back skip (VICTORY_BACK_SKIP_UNREAD=1,
  K=8): skipped S MB in N ranges (fetch and write); demanded later D MB in M ranges (n demands, sync fetch mean/max ms; asked
  by [...]); demoted T ranges; held on the card now ...; left to the pre-demand H; failed fetches F".
  Not covered: title CPU reads outside rings and locks (the F1 bet); copies in PC memory and the in-flight fetch path
  (`VICTORY_CARD_FETCH_INFLIGHT`) never skip (they merge held pieces correctly). Fallback write of a demand when our own
  view refuses (mapping change under way on that page) goes the old way (guards + texture table) and could wait on a lock
  the demanding thread holds: rare, watch for a hang with the switch on.
- Stage 2 (NOT built; notes): the largest card-copy consumer by the stage-0 line decides. Shape: in
  `Host::relay_card_back`, for a skipped piece whose page has a consumer copy with a card side (window or buffer copy over
  the same title bytes; found via `shadow::find`/window table and `inplace::shown_at` for other view addresses), record
  `CopyBufferRegion(consumer.buffer, off_c, producer.buffer, lo, n)` in a list that must run behind the producing flight and
  before the consumer's next use: the landing is too late (the next flights are already queued), so the copy belongs in the
  producer's CardPlan check list (recorded in `card_before`, right behind the flight's lists, like the in-flight fetch) for
  pairs known at record time; the consumer's CPU side must then be told "current from the card" for those bytes (its
  `copied` page stamp) and its CPU side patched at the next demand. Switch `VICTORY_BACK_CARD_TO_CARD` (default 0).
  Risks as listed above (missed consumer at another view address; resource states of the consumer copy).
