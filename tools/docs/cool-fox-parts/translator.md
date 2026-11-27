# cool_fox review, part TRANSLATOR: `crates/victory-shader/**` of the 6 codec_d commits

Read-only review, 2026-11-26. Clone: `C:\GTAVI\Tools\ref-forks\cool_fox` @ `f4b48ad`. Ours: `C:\GTAVI\Tools\victory-dev`.
Paths: `sh/` = `crates/victory-shader/src`, `gpu/` = `crates/victory-gpu/src`. "theirs" = cool_fox HEAD, "ours" = victory-dev.
Nothing was built or run; every statement below is from reading code and `git show`/`git diff`.

## 0. Read first

1. Only **two** of the six commits touch `victory-shader`: `8cc31ab` (lower.rs +50, mod.rs +64, resource.rs +61) and
   `f4b48ad` (lower.rs +34, mod.rs +67, host.rs +46, shader.rs +5, diff_tests.rs +39, valu_tests.rs +334, resource.rs +68).
   `e05d463`, `1ad6e67`, `18bdf79`, `ab75ac3` have no hunk in the crate (checked with `git show --stat <c> -- crates/victory-shader`).
2. **The journal overstates the d16 work.** It says "NVIDIA-valid typed integer lowering (`bufferLoad.i32`/`bufferStore.i32`
   on `VIEW_INTEGER`/`VIEW_SIGNED`)". At HEAD there is **no** `bufferLoad.i32`/`bufferStore.i32` in `sh/emit/lower.rs`
   (the only hits are the optimizer's declarations, `sh/emit/bitcode/opt.rs:1236-1237`). The d16 lowering still goes
   through a **float-typed handle** (`0x409`, `bufferLoad.f32`/`bufferStore.f32`) and picks the integer bits with a
   `select` on a flag bit the host supplies. `VIEW_INTEGER`/`VIEW_SIGNED`/`integer_view` are **base code** (avatarmanz,
   "After VIctory 4243a96a", `sh/emit/shader.rs:34-39`), not codec_d's. codec_d's own contribution is only: the same
   lookup for `RootType::Table` descriptors through a second root SRV (`SLOT_TABLE_STRIDES = 252`).
3. **Their tree has no paired form** (`grep PAIRED|WAVE_PAIRS|half_views|HalfUint` in `cool_fox/crates`: 0 hits). "Port
   NVIDIA Wave32 lowering" is five small generic lowering changes plus two hard-coded program digests that switch the
   barrier form on. Our paired design is the stronger one for an RTX 4080 (section 2).
4. **The resource.rs guard omission is in our tree too** (`sh/resource.rs:227`, `:415`, `:1117-1126`, `:1274-1278`): entry T1,
   20 lines, exact patch below. Whether it costs us the transmission prefilter today is not provable from code (our
   paired form probably catches that program); see T1.
5. No text addressed to AI assistants was found in the crate or in `docs/` (only the README credit lines
   "This fork's changes were made with Claude Code", `README.md:123`, `docs/README-fxceless.md:33`).
6. Every translator source edit changes our cache key (`VICTORY_SHADER_SOURCE_HASH`, `sh/emit/mod.rs:209`): **each entry
   below costs one full shader re-warm**. Batch them.

## 1. Port plan (ordered: independent, highest value first)

### T1. Guarded accesses exclude dead lane edges at a join — coder: TRANSLATOR

- **Theirs:** `sh/resource.rs:232` (`t.at.set(Some(site))`), `:243` (`t.descriptor(inst, op, site)`), `:420-421` (field
  type), `:1123-1146` (`descriptor`), `:1293-1300` (`live`), test `:4112-4150`. Commit `f4b48ad`.
- **What it does:** when a descriptor (or an address term, through `live()`) is traced back through a phi, the tracer
  asks which predecessor edges the lanes that *run the access* can have come by (`only_edge`, with a list `known` of
  facts about those lanes). Before, `known` held only "the lane reaches the block" (`lanes.reach[b]`). Now it also
  holds the access's own EXEC guard (`lanes.guards[b][i]`): a lane whose EXEC bit is off does not execute the access, so
  the edge it arrived by does not count. Typical shape (their test): lanes failing a compare clear EXEC and take
  `s_cbranch_execz` straight to the join without ever building the immediate sampler `s[8:11]`; the join's
  `image_sample_lz` is EXEC-guarded, so only the arm that built the sampler matters. Without the fix the tracer saw a
  join of "sampler constant" and "uninitialised" and refused the program (or the form).
- **Load-bearing code (theirs):**
  ```rust
  // track(): per instruction, not per block
  t.at.set(Some(site));                       // was t.at.set(Some(b)) before the inner loop
  let source = t.descriptor(inst, op, site).map_err(fail)?;
  // Tracer
  at: std::cell::Cell<Option<Site>>,
  fn descriptor(&self, inst: &Inst, op: &Operand, at: Site) -> ... {
      let mut known = Vec::new();
      if let Some(l) = self.lanes {
          known.push((l.reach[at.0], true));
          if let Some(guard) = l.guards[at.0][at.1] { known.push((guard, true)); }
      }
  fn live(&self, mut t: Term) -> Term {
      ...
      let mut known = vec![(lanes.reach[at.0], true)];
      if let Some(guard) = lanes.guards[at.0][at.1] { known.push((guard, true)); }
  ```
- **Ours has the same omission. Proof:** `sh/resource.rs:227` `t.at.set(Some(b));` (outside the instruction loop);
  `:237` `t.descriptor(inst, op, b)`; `:415` `at: std::cell::Cell<Option<usize>>`; `:1117` `fn descriptor(.., at: usize)`,
  `:1126` `let mut known = Vec::from_iter(self.lanes.map(|l| (l.reach[at], true)));`; `:1274-1278` `live()` with
  `vec![(lanes.reach[at], true)]`. `Structured::guards` exists in ours (`sh/structure.rs:419`,
  `pub guards: Vec<Vec<Option<Bool>>>`), so the patch applies as written: 5 edits, about 20 lines.
- **Where it goes in ours:** exactly those five places; move the `t.at.set` into the inner loop (`:228-229`), change the
  field type and the two functions. `t.at.set(None)` at `:337` stays.
- **Does `CS_AlphTransmissionPrefilterMip_610a2fad` (`2da4beb386509b77`) translate in ours today?** Code path: in ours the
  wave64 retry builds the "without early-outs" form with `resource::track(..).ok()?` (`sh/emit/mod.rs:561-567`), so a
  track failure silently removes that form (no line even with `VICTORY_WAVE_WHY`). Theirs then had nothing left (no
  paired form, wide off) and the program was dropped. Ours goes on: "without skips", deep skips, local-wave forms, and
  finally the **paired form** on the *original* program and its original `res` (`sh/emit/mod.rs:450-472`, `:889-`), which
  does not need the early-out-free form. So ours most likely translates it, in the paired form, provided its thread
  group is a multiple of 64 (`:902`). Not proven: nothing was run. **Check before blaming glowing eyes on this:**
  search a run log for `2da4beb386509b77` / `CS_AlphTransmissionPrefilterMip` next to "refused"/"unsupported"/"host
  pipeline" lines, or run the offline translate tool on that hash with `VICTORY_WAVE_WHY=1` and read which
  `WAVE64-RETRY` line wins. If it is absent from the refusals, T1 does not explain our glowing eyes / black faces.
- **Why port it anyway:** it is a correctness fix of the tracer for every stage (pixel programs with a guarded sample at
  a join too), it can only make more programs/forms translate (a guard only removes edges), and it is the precondition
  for the earlier, cheaper forms to be chosen before the paired one.
- **Switch:** theirs none. Ours: none recommended inside the tracer (it is a proof step); if an A/B is wanted,
  `VICTORY_GUARD_EDGES=0` reading a `OnceLock<bool>` in `descriptor()`/`live()` and skipping the two pushes
  (it is auto-added to `VICTORY_SHADER_SWITCHES`, so the cache separates the two).
- **Verify:** port their unit test `a_guarded_access_excludes_dead_lane_edges_at_a_join` (`sh/resource.rs:4112`; it uses
  the test helpers `lanes()` and `param()`, both present in ours at `sh/resource.rs:2406`, `:2410`). In a run: fewer
  "unsupported" translation lines; with T5 the form that failed is named.
- **Picture:** improves or neutral. **Cache:** translator key changes (full re-warm); programs that now take an earlier
  form get different DXIL.
- **Author's claim:** "RTX 5090 `translate --jobs 1 --validate --pipelines` on 154 VFX/particle/eye shaders emitted &
  validated 43/43 graphics + 96/111 compute pipelines (recovering `CS_AlphTransmissionPrefilterMip`, 0 regressions)";
  "`victory-shader` lib suite 281 passed". No frame-time number.
- **Size/risk:** S (20 lines + 40-line test, 1 file) / low. **Addresses:** possibly glowing eyes, black squares in
  particles (`ptfx_PS_Transmission_aba1527f`), transmissive alpha; no bound.

### T2. Integer view kind per heap slot, for d16 accesses our halves port skips — coder: TRANSLATOR + HOST

This is the fix for item **A2** of `docs/silent-defects-audit.md` ("halves fix skips some places … extend to heap-picked
views (needs per-slot format); medium") — already listed there; this entry is the port-ready form of it, using their
mechanism.

- **Theirs (translator):** `sh/emit/shader.rs:26-41` (constants), `sh/emit/lower.rs:7503-7576` (`slot_stride`,
  `integer_view`, `slot_word`), `:11898-11928` (d16 store), `:12066-12088` (d16 load), `sh/emit/mod.rs:124-125` (re-export).
  Base code (avatarmanz, after VIctory 4243a96a) + codec_d `f4b48ad` (the `Table` arm and `SLOT_TABLE_STRIDES`).
- **Contract between shader and host (exact):**
  - Root SRV `t255, space1` (`SLOT_STRIDES = 255`, `ROOT_CBV_SPACE = 1`): raw buffer of `SLOT_STRIDES_COUNT = 1 << 21`
    dwords, dword `i` describes the buffer view at **view-heap slot `i`**. Its presence in `HostShader::root_srvs` also
    tells the host to run the whole-heap buffer sweep (`heap_buffers`).
  - Root SRV `t252, space1` (`SLOT_TABLE_STRIDES = 252`): **the same buffer, same indexing**, but its presence does *not*
    trigger the sweep (`gpu/draw.rs:1750-1767`, `heap_strides` `gpu/draw.rs:2125-2141`). Used when the shader only
    inspects views that came through `RootType::Table`.
  - Dword layout: bits 0-29 bytes per element (`VIEW_STRIDE_BITS = (1 << 30) - 1`; structured: stride, raw: 1, typed:
    the format's size; 0: none / not a buffer), bit 31 `VIEW_INTEGER` (typed UINT or SINT format), bit 30 `VIEW_SIGNED`
    (with bit 31: SINT). Kind bits only for typed views (`stride == 0 && !raw`).
  - Format lists (DXGI numbers; `gpu/descriptor/host.rs:1172-1173`): `VIEW_INTEGER` for 3, 7, 12, 17, 25, 30, 36, 42, 50,
    57, 62; `VIEW_INTEGER | VIEW_SIGNED` for 4, 8, 14, 18, 32, 38, 43, 52, 59, 64. (25 = R10G10B10A2_UINT.)
  - Index the shader uses: `heap_index(load, first, false)` = (descriptor byte address of the scalar load) >> 5, clamped
    to `SLOT_STRIDES_COUNT - 1` (`lower.rs:7548-7557`), read with `rawBufferLoad.f32` (op 139) at byte `slot << 2`. For a
    table the address is the table's root constant (the host slot the table was copied to, times 32) plus the place:
    so the host must have written the word **at the dynamic host slot of the table copy**, not at the title's slot.
  - Lowering, **only** for `buffer_load_format_d16_*` / `buffer_store_format_d16_*` on a typed access (`if typed {
    self.integer_view(src)? }`; the journal: applying it to non-d16 typed loads "caused shader regressions"):
    - store, per channel `x` (16 bits from the VGPR half): `w = select(int, select(signed, sext16(x), x & 0xFFFF),
      bits(legacyF16ToF32(x)))`, then `bufferStore.f32(handle 0x409, …, as_float(w))`;
    - load, per channel `f`: `h = select(int, bits(f) & 0xFFFF, legacyF32ToF16(f))`.
    The handle stays float-typed; an integer view read through it is taken to hand the bits over unconverted
    ("whose raw bits RTX and WARP pass bit-exact", `lower.rs:11905-11909`). That is undefined in D3D12 (our audit item
    A3: "works today on NVIDIA; fragile").
- **Theirs (host counterparts, for the HOST coder):**
  - `gpu/descriptor/host.rs:1156-1186` `view_stride_and_kind`, `view_stride_word`; `:1200-` `view_elements`;
  - `:2762-2774` `write_stride(index, word)` (volatile dword write into the mapped upload buffer);
  - callers that fill/clear the word for **dynamic** slots (`f4b48ad`): `:2549`, `:2745-2760`, `:2800`, `:2833-2850`
    (`make_listed`: `ListView.stride_word` cached with the view, `:5787-5792`), `:2875`, `:2962`, `:2972`, `:3007`, `:3037`,
    `:3095` (raw placeholders write 1, nulls write 0); test `dynamic_slots_populate_and_clear_stride_table` `:5704-5770`;
  - `gpu/draw.rs:1750-1767` binding of slot 252, `:2125-2141` `heap_strides` (keeps the buffer alive on the list with
    key `gpu | 2`); `gpu/pipeline/host.rs:522-528` slot 252 stays a root descriptor (not a bounded one-view table).
  - Base dependency ours lacks: their dynamic table copies go through `make_listed`/`ListViews` (a per-recording view
    cache; 0 hits for `make_listed` in ours). **Not needed**: ours copies table views at `gpu/draw.rs:4027-4035`
    (`host.make_at(index, &view)`), the word can be written right there.
- **Ours today (what we have instead):** static, per program: `places_read_as_halves` (`sh/emit/halves.rs:104-147`) finds
  table places every access through which moves halves; those are lowered with a **uint handle** (`0x405`,
  `bufferLoad.i32`/`bufferStore.i32`, `sh/emit/lower.rs:10273-10285`, `:10502-10546`, `:10603-10609`, `:10676-10699`) and named
  to the host (`HostShader::half_views`), which shows a FLOAT/SINT 16-bit view as its UINT twin
  (`gpu/descriptor/host.rs:3872-3912`, `gpu/draw.rs:3756-3766`, `:4027-4035`). Grep in `victory-dev/crates` for
  `VIEW_INTEGER`, `VIEW_SIGNED`, `integer_view`, `SLOT_TABLE_STRIDES`, `heap_strides`: **0 hits**. Our strides table
  holds the stride only (`gpu/descriptor/host.rs:2060-2066`), and only for title slots.
- **Precise comparison:**

  | Case | Ours | Theirs |
  |---|---|---|
  | d16 through a table place used only for halves, view UINT | exact and valid D3D12 (uint handle) | exact on RTX/WARP by their test, via float handle (undefined behaviour) |
  | same, view FLOAT (R16*_FLOAT) holding integer bit patterns | exact: UINT twin | **not fixed**: F16→F32→F16; denormal halves (small integers) are flushed to 0 on NVIDIA — their test deliberately uses "normal finite FP16 values that round-trip cleanly" for the FLOAT case (`valu_tests.rs:2204-2210`) |
  | same, view SINT | exact (UINT twin of SINT; 16 bits pass) | exact (sign-extends on store, low 16 bits on load) |
  | non-d16 format load read only as halves (`v_cvt_f32_f16`, `v_fma_mix*`) | exact (`format_load_read_as_halves`) | float handle, driver passes bits (test `typed_non_d16_table_load_preserves_integer_bits`) |
  | d16 through a view **picked from the heap** (`RootType::ResourceHeap`) | **not covered** (`table_place` needs `RootType::Table`, `lower.rs:6340`): F32ToF16 of an integer = 0 on load, converted floats on store | covered (bit 31/30 of the `SLOT_STRIDES` word) |
  | d16 through a table place that another access also uses otherwise (`other`) | **not covered** (`halves.rs:145`) | covered (`SLOT_TABLE_STRIDES`) |
  | d16 through a table the program also indexes at run time (`picked`, `a.dynamic`) | **not covered** (`halves.rs:128-133`, `:145`) | covered (`heap_index` follows the load's `soffset`) |
  | descriptor chosen by control flow (`Source::Phi/Select/Lanes`) | not covered | not covered (`slot_word` takes `Source::Loaded` only) |
  | loads vs stores | both | both |
  | stride lookup for table views | not needed (used-resource entry) | not used either: `slot_stride` passes `allow_table = false` (`lower.rs:7504`); the table word is read for its kind bits only |

  So theirs covers three place classes ours leaves on the float path (heap-picked, mixed-use place, run-time indexed
  table), for views that really are UINT/SINT. Ours covers the FLOAT-view case theirs does not. The two are
  complementary; the port is "keep ours, add their per-slot kind bit as the fallback where `halves == false`".
  Audit run 92 says the game's views at half places are already integer views ("the host made 0 unsigned twins"), which
  is exactly the case their fallback handles.
- **Bearing on open defects:** black NPC faces / frozen-or-wrong face mesh: plausible if a face or wrapped-mesh program
  reaches its UINT view by one of the three uncovered routes (their test comment names `compute_graph_ops`,
  `ped_standard_head_cs`, `CS_BlendshapeTranslationsHSSGraphOutputExtract`, `CS_PedStandardSkinning_*`). Stippled cloth
  and bare palms: no evidence in these hunks that they are d16 problems; do not expect T2 to fix them. Do step (a)
  first, it decides whether (b)-(d) are worth a re-warm.
- **Where it goes in ours, as four steps:**
  - (a) *Count first* (TRANSLATOR, 15 lines): at `sh/emit/lower.rs:10278-10285`, when `typed && d16 && place.is_none()`,
    record the program in a new `Lowered`/`HostShader` note (or `eprintln!` under `VICTORY_WAVE_WHY`-style switch
    `VICTORY_HALF_WHY=1`) with the root type of `src` (ResourceHeap / Table-mixed / Table-picked / other). The host logs
    it once per pipeline next to `note_half_views` (`gpu/draw.rs:5035`). If the list is empty for the face/cloth
    programs, stop here.
  - (b) TRANSLATOR `sh/emit/shader.rs:26-28`: add `VIEW_STRIDE_BITS`, `VIEW_INTEGER`, `VIEW_SIGNED` as above and
    `SLOT_TABLE_STRIDES`. **Do not use 252 in ours: `SLOT_HEAP_MARKS = 252` already (`sh/emit/shader.rs:43`, a root UAV;
    different register class, but the host tells "ours" slots by number, `gpu/pipeline/host.rs:572-577`). Use 251.**
    Export from `sh/emit/mod.rs:124-125`.
  - (c) TRANSLATOR `sh/emit/lower.rs:6543-6585`: split into `slot_word(src, allow_table)` / `slot_stride` (mask with
    `VIEW_STRIDE_BITS` — required as soon as the host sets the high bits, or every heap-picked raw read multiplies by a
    stride with bit 31 set) / `integer_view` as theirs. In the d16 store (`:10502-10526`) and load (`:10666-10692`), in
    the `!halves` arm only, add their two `select`s. `halves` places keep our uint path.
  - (d) HOST: `gpu/descriptor/host.rs:884-` make `view_stride` return the word with kind bits (copy
    `view_stride_and_kind`), `:2060-2066` write `stride | kind`; `gpu/draw.rs:4027-4035` after `host.make_at(index, ..)`
    write the word for the dynamic `index` (and 0 on `make_null`/`constant_at` there) — needs a `pub(crate) fn
    write_stride` on the host heap as theirs `:2762`; `gpu/draw.rs:2092` add the slot-251 arm binding
    `host.strides_gpu()` without `heap_buffers` (copy `heap_strides`); `gpu/pipeline/host.rs:576` add the slot to `ours`.
    `gpu/draw/probe.rs:630` reads the table too: mask there.
- **Switch:** theirs none (always on). Ours: `VICTORY_VIEW_KINDS` default **0** for the first A/B, then 1; read in the
  translator (so it lands in the cache key by itself) — when 0, `integer_view` returns `None` and no slot-251 SRV is
  requested; the host side can write the bits unconditionally (harmless once `slot_stride` masks).
- **Verify:** port `typed_d16_table_uav_roundtrip_uint_sint_and_float` and
  `typed_non_d16_table_load_preserves_integer_bits` (`sh/emit/valu_tests.rs:2062-2393` theirs; they need T6's
  `View::TypedUav` and the helpers `mubuf_idxen`, `s_load_dwordx4_off`, `build_table_typed_cs`; in ours the
  `HostShader` literal needs `half_views: Vec::new()` and our other extra fields, see `sh/emit/valu_tests.rs:227`). **Run
  the first test on the RTX 4080 before anything else**: it is the only evidence that a UINT view read through a
  float handle passes bits on our driver; if it fails, do the valid variant instead (a second, uint-typed handle and a
  branch, which is what the journal says they tried and backed out of). Host test:
  `dynamic_slots_populate_and_clear_stride_table`. In a run: the step-(a) list shrinks to the phi-chosen cases;
  face animation / wrapped meshes in the affected shots.
- **Picture:** improves or neutral. **Cache:** key changes; DXIL changes only for programs with a d16 access outside a
  halves place (others byte-identical apart from the masked stride read in heap-picking programs).
- **Author's claim:** no number. "restricting integer branching to `buffer_load_format_d16_*` and
  `buffer_store_format_d16_*` restored clean shading while keeping `VICTORY_TRANSLATOR` cache-compatible with
  `tools/cache-pareto` (`made=0`)" (RTX 5090). Suspicious as worded: their `VICTORY_TRANSLATOR` is a hash of every file
  under `victory-shader/src` (`crates/victory-gpu/build.rs:22-44`), so any edit changes it; "made=0" can only describe
  a cache rebuilt after the edit, or identical DXIL, not an unchanged key.
- **Size/risk:** M (translator ~70 lines, host ~60 lines, tests ~380 lines) / medium (undefined-behaviour read; mask
  must land together with the host bits). **Prerequisites:** T6 for the tests. **Addresses:** black/frozen NPC faces,
  wrapped meshes (audit A2); no bound.

### T3. Refuse a wave64 result on a wave32 host in the translator — coder: TRANSLATOR

- **Theirs:** `sh/emit/mod.rs:659-669`, `:683` (`8cc31ab`):
  ```rust
  let supported = |(shader, wave): (HostShader, Option<u32>)| {
      if wave == Some(64) && lower::host_max_wave() == 32 {
          Err(ShaderError::Unsupported { offset: 0, what: "no wave32 lowering for this wave64 program" })
      } else { Ok(shader) }
  };
  let original = original.and_then(supported);
  ```
- **Ours:** `sh/emit/mod.rs:474` `r.map(|(shader, _)| shader)` and `:763`: a program that no form brings to 32 lanes is
  returned as a 64-lane shader; grep `no wave32 lowering` in ours: 0 hits; grep `wave_size|WaveSize|wave64` in
  `gpu/pipeline/*.rs`, `gpu/draw.rs`: 0 hits, so the host learns it only when the driver refuses the pipeline.
- **Change:** in `emit_any` (`:450-474`) after the paired retry, map `Ok((_, Some(64)))` with `host_max_wave() == 32` to
  that `Unsupported`. Gives one clear reason line per such program instead of a driver PSO failure, and lets the
  threading retry at `:752-763` run for it (it runs for `Unsupported` only).
- **Switch:** none needed (the result was unusable anyway). **Verify:** count of "host pipeline refused" lines for
  compute goes to the count of "no wave32 lowering" lines; nothing else changes. **Picture:** neutral. **Cache:** key
  changes. **Claim:** none ("Wave64-to-Wave32 translation warnings remain in the logs"). **Size/risk:** S (10 lines) /
  low. **Addresses:** tooling; removes driver-side compile attempts of unusable shaders (pipeline workers).

### T4. Four generic lowering refinements from "Wave32 lowering" — coder: TRANSLATOR (optional, after a count)

All `8cc31ab`. Each lets an *exchange* form (two host waves that swap halves) take a program it stopped at before.
In ours these programs fall through to the paired form, which is exact, so the gain is only for programs the paired
form refuses (thread group not a multiple of 64, `sh/emit/mod.rs:902`) — count those first with `VICTORY_WAVE_WHY=1`
(T5). Port only if that list is not empty.

| ID | Theirs | What | Ours (anchor) |
|---|---|---|---|
| T4a | `sh/emit/lower.rs:550`, `:563-565`, `:576-585`, `:1699-1701` | `lane_uniform` gets `ordered_lds`: a `ds_read_b32` (not GDS, one dword) whose address VGPR is lane-uniform gives a lane-uniform result, when LDS is split and group sync is on (`split_lds && group_sync() != 0`). The guard check moves before the match so it covers the new arm. | `sh/emit/lower.rs:548-595`, call `:1462` (0 hits for `ordered_lds`) |
| T4b | `sh/emit/lower.rs:1188-1195` | `reads_mask_as_data`: an `Sop`/`Valu` instruction that reads an SCC which is the "some lane" bit of a mask operation (`st.scc_some`) counts as a data read of the mask, even when the mask has no scalar-dword consumer. | `sh/emit/lower.rs:903-` (no such arm) |
| T4c | `sh/emit/lower.rs:6047-6056` | `scc_in`: an SCC in `st.scc_some` that has no value yet (`None`/`Val::Missing`) is computed on demand with `self.scc_some(v)` and cached as `Val::Bit`. | `sh/emit/lower.rs:5903-5909` (`self.get(v)?` only; `scc_some` is called from one place, `:11330`) |
| T4d | `sh/resource.rs:143`, `:233-235`, `:2311-2326` (`Moved` made `pub(crate)`, `plain_lane`), `:2329-2346`, `:2350-2400`; `sh/emit/lower.rs:7726-7734` | `restored_lanes`: a `v_readlane_b32` of a constant lane whose VGPR was written at that lane by a `v_writelane_b32` **in the same block** is lowered as the scalar that was written (`write_i(inst, word, None)`), without reading the vector ("the previous contents of other lanes can be unknown (a reduction scratch)"). Also hardens `restored`/`spilled`: both instructions must be plain (`abs/neg/opsel/clamp/omod == 0`, no SDWA/DPP) and the lane below the wave size (64 or 32). | `sh/resource.rs:2257-2315` (no `block` parameter, no `plain_lane`, no lane bound), `sh/emit/lower.rs:6816-6821` (no `restored_lanes`) |

- The **hardening half of T4d is worth taking alone** (10 lines): ours follows a `v_writelane_b32` with operand
  modifiers or a lane index ≥ wave size as if it were a plain spill (`sh/resource.rs:2296-2303`), which can name a wrong
  descriptor source. Low risk.
- **Constant rename:** `DIVERGENT_MASK_EXCHANGE` (`sh/emit/lower.rs:426`, used `:3549`, `:4250`, `mod.rs:559`) names the
  string "a lane mask exchanged where not every lane runs" (ours: literal at `sh/emit/lower.rs:3318`, `:4034`). Only
  needed by `driver_loop` (N2, not ported); skip.
- **Switch:** none in theirs. Ours: one switch `VICTORY_WAVE_REFINE=0` gating a/b/c/d-lowering for the A/B.
- **Verify:** the `WAVE64-RETRY` line that wins per program moves from "paired lanes"/"APPROXIMATE" to an earlier
  exact form; lib test suite. No test of theirs covers a–d specifically (none added in `8cc31ab`).
- **Picture:** neutral if the paired form took the program before; improves where an APPROXIMATE local-wave form did.
  **Cache:** key changes. **Claim:** none. **Size/risk:** S each (15-70 lines) / medium (they change which form wins
  for an unknown set of programs). **Addresses:** correctness of compute passes outside the paired form; no bound.

### T5. Say why a wave64 retry form was not built, and which branches went wave-wide — coder: TRANSLATOR (tooling)

- **Theirs:** `sh/emit/mod.rs:488-527` (`form(label, p)` prints `WAVE64-FORM {label}: ir returned None | structure
  failed: {e} | io::raise failed: {e} | resource::track failed: {e}` under `VICTORY_WAVE_WHY`), `:731-746` (`WAVE64-WIDE
  n branches taken wave-wide: [B12(s_cbranch_scc1,BACK), …]`). Commit `f4b48ad`. This is how they found the T1 bug.
- **Ours:** `sh/emit/mod.rs:561-567` (`.ok()?` three times, silent), `:859` (count only). 0 hits for `WAVE64-FORM`.
- **Change:** copy both; stderr only, under the existing `VICTORY_WAVE_WHY`. **Switch:** existing. **Verify:** lines
  appear in the offline translate output. **Picture:** none. **Cache:** key changes (source hash) although output is
  identical — land it together with T1. **Size/risk:** S (45 lines) / none. **Addresses:** tooling.

### T6. Test host: typed buffer UAV with contents; strides root view in the PC-diff test — coder: TRANSLATOR (tests)

- **Theirs:** `sh/emit/host.rs:1007-1012`, `:1288-1327` (`View::TypedUav { data, format, elem_bytes }`: default-heap buffer
  filled from an upload buffer, typed UAV of `bytes / elem_bytes` elements, read back); `sh/emit/diff_tests.rs:1231-1268`
  (`translated_compute_matches_pc` builds a `view_strides` vector — raw 1, `HalfUav` 8, typed 16, structured `s`,
  `Stride::Format` bytes | kind — and hands it to root SRV 255 or 252 instead of asserting every root view is slot 4/5).
  Commit `f4b48ad`.
- **Ours:** `sh/emit/host.rs:999` has `HalfUav` only (0 hits `TypedUav`); `sh/emit/diff_tests.rs:1219-1226` still asserts
  `slot == 4 + srv`.
- **Change:** add the variant after `:999` and its arm near `:1232`; the diff_tests hunk only together with T2 (it
  references `VIEW_INTEGER`, `SLOT_TABLE_STRIDES`). **Size/risk:** S (46 + 39 lines) / none (test code). Prerequisite of
  T2's tests.

## 2. "Port NVIDIA Wave32 lowering" (8cc31ab) versus our paired wave64 design

What `8cc31ab` actually lowers (all verified in the diff): T3 (refusal), T4a–d (four refinements), and two policy
changes in `sh/emit/mod.rs`:

- **`scene_kernel`** (`mod.rs:542-551`): `VICTORY_WAVE_WIDE` unset now means "on for two programs": compute, thread group
  of exactly 64, container digest `58 1d ab 2c d8 bc f2 2e 5c 96 4b 98 e9 03 48 d1` or
  `24 31 bb e7 dc 15 4a d7 59 d2 0b c9 54 82 9c 1c` ("Both archived kernels use one logical wave per group and have
  validated wave32 PSOs on RTX"). Explicit `VICTORY_WAVE_WIDE=0/1` still wins.
- **`driver_loop`** (`mod.rs:555-559`, `:579`, `:587`, `:698`, `:761-783`): with the switch unset, a container with an `ACCC`
  part and no `DXIL` part whose plain translation stopped with `DIVERGENT_MASK_EXCHANGE` gets `widen(..,
  active_loop_only = true)`: accepted only if exactly one branch is taken wave-wide, it is a backward
  `s_cbranch_scc1` directly after `s_andn2_wrexec_b64` (the "repeat while some lane is unfinished" latch), and nothing
  else was widened.

Both use `widen`, the form that puts **two group barriers per widened branch** inside loops. Our comment at
`sh/emit/mod.rs:582-587` records that this form "lost the card" in runs 121-124 (`ptfx_CS_lightAtlas_Sprite`, `_Trail`),
and their own journal keeps it off in general ("`VICTORY_WAVE_WIDE` intentionally kept off due to known loop-widening
TDR risk"). They need it because they have nothing else: no paired form.

Ours (`sh/emit/mod.rs:450-472`, `:595-618`, `:876-`, `lower::with_paired_lanes`, `structure::structure_wide`): each console
wave is run by **one** 32-lane host wave, each thread carrying two console lanes, every per-lane branch taken by the
whole wave as the console takes it, barriers only at entry. It is exact, has no barrier inside loops, is tried before
any approximate form (36 of 39 formerly approximate programs take it) and last for everything else.

**Verdict for an RTX 4080 SUPER (32-lane waves only):** theirs is **worse** as a design (barrier form with a known
device-loss history, enabled per hard-coded digest; programs outside those cases are dropped or run approximate),
**equal** in the small refinements (T4), which are independent of the form. Do **not** port `scene_kernel` or
`driver_loop` (section 3, N1/N2). Nothing in `8cc31ab` is a speed claim: no number is given for it.

## 3. Hunks not worth porting, and why

| ID | Hunk (theirs) | Why not |
|---|---|---|
| N1 | `scene_kernel` digests, `sh/emit/mod.rs:542-551` | Turns the barrier (`widen`) form on by default for two programs; our paired form takes wave64 compute exactly without barriers; the widen form lost the card in our runs 121-124. If either digest shows up among our refused programs, test `VICTORY_WAVE_WIDE=1` on it alone first. |
| N2 | `driver_loop` + `active_loop_only`, `sh/emit/mod.rs:555-559`, `:761-783`; `DIVERGENT_MASK_EXCHANGE` constant | Same form, same risk (their own journal: "known loop-widening TDR risk"); in ours such a program goes to the paired form. |
| N3 | `SLOT_TABLE_STRIDES = 252` as a number | Collides with our `SLOT_HEAP_MARKS = 252` (`sh/emit/shader.rs:43`); use 251 (T2b). |
| N4 | The comment rewrite at `sh/emit/lower.rs:11901-11909` | Text only. |
| N5 | `gpu/pipeline/make.rs:44-86` `left_out` (`8cc31ab`, named in the task; HOST scope) | Skips `CS_RaytracingResolve` (~1,000 programs) unless `VICTORY_RT_RESOLVE=1` and `CS_ScreenProbes_ActivateRays` unless `VICTORY_ACTIVATE_RAYS=1`, when `VICTORY_BVH_BUILDER` is not 1. Already listed and declined, see `docs/fork-review-shortlist.md` (row 6, "Leaving out RT programs"). Once our builder is on these are passes of the picture: skipping them is **not acceptable: trades picture**. This commit has **no** wave32 handling in `make.rs`; the wave32 refusal lives in the translator (T3). |
| N6 | `gpu/pipeline/make.rs:88-101`, `:117-121`, `:251-259` `scene_input` (six shader hashes + `BAKE_PIXEL_SHADERS` started at `REQUESTED` priority on a cold cache); `worker_count_for(.., limited_ram)` `:474-484` | HOST scope, not translator; no picture trade. Left to the host part. Hashes recorded under Game facts. |

## 4. Other points in these hunks

- **Cache key.** Theirs: `VICTORY_TRANSLATOR` = blake3 of every file under `victory-shader/src` plus `Cargo.lock`,
  `rust-toolchain.toml`, `shader_cache.rs` (`crates/victory-gpu/build.rs:22-44`, base code, untouched by codec_d). Ours:
  `VICTORY_SHADER_SOURCE_HASH` + every `VICTORY_*` switch the translator reads + host wave/heap limit/loop limit/group
  sync/shader-opt (`sh/emit/mod.rs:207-226`). Consequences: (1) every T-entry re-warms the whole cache once; (2) new
  switches read inside `victory-shader` separate their caches automatically; (3) `HostShader` gains no field in T1–T5,
  so `gpu/shader_cache.rs` (`:250`, `:324`) needs no format change.
- **`HostShader` shape differs:** theirs has no `half_views`; their test helper builds the struct literally
  (`valu_tests.rs:2118-2128`), so ported tests must add our fields.
- **Tests added by codec_d in scope:** `a_guarded_access_excludes_dead_lane_edges_at_a_join` (resource.rs),
  `typed_d16_table_uav_roundtrip_uint_sint_and_float`, `typed_non_d16_table_load_preserves_integer_bits`
  (valu_tests.rs; both run on hardware and WARP, serialised by `ONE_AT_A_TIME`, which ours has at
  `sh/emit/valu_tests.rs:245`), and the `translated_compute_matches_pc` change. `8cc31ab` adds no test for its lowering
  changes.
- **Suspicious / to distrust:** (1) journal wording on `bufferLoad.i32` (not in the code); (2) the "cache-compatible
  (`made=0`)" claim (T2); (3) the journal's frame-time medians (64.2 → 54.8 ms, 57.7 ms, 54.3 ms; RTX 5090, "normal
  Pareto rendering settings", runs "progressed at different rates … directional") belong to the residency work, not to
  any translator hunk; (4) T2's read of an integer view through a float handle is undefined behaviour, verified by them
  only on an RTX 5090 and WARP.

## 5. Game facts found in the hunks, journal and tests

- `alphaResolve:CS_AlphTransmissionPrefilterMip_610a2fad`, shader hash `2da4beb386509b77`: fills the transmission buffer
  (their address `0x17dff443d70`); when it does not run the buffer stays zero and `ptfx_PS_Transmission_aba1527f`
  (dual-source blend) and transmissive eye/alpha shaders output black squares / corrupted blending. Its code has an
  EXEC-guarded sample at a join whose skipped arm never builds the immediate sampler. (`docs/bug-journal.md:19`,
  `sh/resource.rs:4112-4117`.)
- `compute_graph_ops` ("facial lip-sync & wrapped submesh solver") binds `R16G16B16A16_UINT` / `R16_UINT` UAVs through a
  descriptor table and moves FP16 bit patterns with `buffer_load_format_d16_xyzw` (MUBUF op `0x83`) and
  `buffer_store_format_d16_xyzw` (op `0x87`). `ped_standard_head_cs` keeps its descriptors in a table at `s4` likewise.
  (`sh/emit/valu_tests.rs:2076-2078`, `:2138-2146`, `:2159-2162`.)
- `CS_BlendshapeTranslationsHSSGraphOutputExtract` reads `HSSGraphOut` (`R16_UINT`) with the non-d16
  `buffer_load_format_x` (op `0x00`); `CS_PedStandardSkinning_*` reads bone indices the same way.
  (`sh/emit/valu_tests.rs:2279-2282`.)
- "the face network keeps its halves in UINT views; its output was noise" (VIctory 4243a96a; `sh/emit/lower.rs:11901-11903`).
- Console loop latch: `s_andn2_wrexec_b64` followed by a backward `s_cbranch_scc1` = "restrict EXEC to the unfinished
  lanes, repeat while either half still has work" (`sh/emit/mod.rs:762-766`).
- Reduction scratch: a VGPR lane written with `v_writelane_b32` and read back with `v_readlane_b32` in the same block
  while its other lanes are undefined (`sh/emit/lower.rs:7727-7728`).
- Two compute programs with thread groups of 64 ("scene kernels", "archived kernels", one logical wave per group):
  container digests `581dab2cd8bcf22e5c964b98e90348d1` and `2431bbe7dc154ad759d20bc954829c1c` (`sh/emit/mod.rs:545-549`).
- Driver compute route: containers with an `ACCC` part and no `DXIL` part (`sh/emit/mod.rs:556-557`).
- `ptfx_CS_lightAtlas_Sprite` and `_Trail`: lost the card when moved to the barrier form (`sh/emit/mod.rs:535-538`, also
  in ours).
- Host scope, recorded for completeness: "Without BVH, ~1,000 CS_RaytracingResolve shaders shade no hits"
  (`gpu/pipeline/make.rs:67-68`); `CS_ScreenProbes_ActivateRays` = ray activation; first-scene producer shader hashes
  `970c4fbb3b017c0e`, `068c90badc86f1fd`, `e4a7e088c7780ce8`, `ebe4e53f031f5249`, `2cc4c197ee29e291`, `369cbe906ea72764`
  (`gpu/pipeline/make.rs:90-98`).
- NVIDIA dynamic descriptor pool of 262k slots exhausted when completed submissions keep their slots pinned 16
  submissions (`docs/bug-journal.md:19`; host scope).

## 6. Coverage

Commits examined for `crates/victory-shader`: 6 of 6 (`e05d463`, `1ad6e67`, `18bdf79`, `ab75ac3`: no hunk in the crate;
`8cc31ab`: 3 files; `f4b48ad`: 7 files).

| File | Hunks (8cc31ab + f4b48ad) | Read | Where covered |
|---|---|---|---|
| `sh/emit/lower.rs` | 9 + 4 | all | T2, T4a–d, N2, N4 |
| `sh/emit/mod.rs` | 8 + 4 | all | T3, T5, N1, N2, T2 (re-export) |
| `sh/resource.rs` | 6 + 6 | all | T1, T4d |
| `sh/emit/shader.rs` | 0 + 1 | all | T2, N3 |
| `sh/emit/host.rs` | 0 + 2 | all | T6 |
| `sh/emit/diff_tests.rs` | 0 + 1 | all | T6 |
| `sh/emit/valu_tests.rs` | 0 + 1 (334 lines appended at `:2060`; the file is not new) | all | T2 (tests), Game facts |
| `docs/bug-journal.md` | 19 lines | all | sections 0, 2, 4, 5 |

Host counterparts read for the contract (not reviewed hunk by hunk; they belong to the host part):
`gpu/descriptor/host.rs:1150-1215`, `:2760-2775` and the list of `write_stride` call sites from `git show f4b48ad`;
`gpu/draw.rs:1735-1790`, `:2115-2145`; `gpu/pipeline/host.rs:510-540`; `gpu/pipeline/make.rs` (`8cc31ab`, whole diff);
`crates/victory-gpu/build.rs`.

Not read: nothing in scope. Not done (by rule): no build, no test run, no log search — every "translates in ours"
statement is from the code path only.
