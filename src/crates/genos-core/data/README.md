# Genos factory voice catalog

`genos-factory-voices.json` is compiled into `genos-core` and the desktop executable.
It contains the 1,711 factory voice entries (including the DrumKit category) from
Genos v2.00 English Data List, November 2019, pages 3–14. MIDI program numbers are
stored as 0–127; the UI displays 1–128.

The supported source PDF SHA256 is
`6022804163e4548abfd26db1889307d78f913297a3cc78b6cf6905bb3060a9bd`.
The data file includes its source fingerprint and a checksum over serialized rows.
The voice catalog integration tests compare every embedded entry against both
the PDF extractor and the independent `oracle-voices.json` extraction.

To regenerate from the repository's verified source PDF, run from `src/`:

```powershell
cargo run -p genos-core --example build_factory_voices -- ../docs/development/voice-catalog/genos_en_dl_f0.pdf crates/genos-core/data/genos-factory-voices.json
cargo test -p genos-core --test voice_catalog
```

End users need no PDF, sidecar data file, import step or network connection.
An optional validated local import still takes precedence; missing or damaged
local data falls back to the embedded catalog without overwriting that data.

`genos-compatibility-voices.json` adds 745 address-backed XG/GM and GM2 entries
from the inspected StyleMagic YA `instrdef/GENOS.idl` (SHA256
`51e6586aa296ebb38f2366061f24793e303cbc1374fc4f024fa7150feec8fbc9`).
The complete in-memory v2 catalog has 2,456 entries. Ten named separator rows in
that reference are excluded. Panel entries retain their official Yamaha names.
Compatibility entries retain the exact reference name and bank section, with
`page: 0` because they were not extracted from the panel Data List pages.
No Genos2 or other model definitions are merged. GM2 kits are selectable programs;
they do not expand the separate native per-key editing compatibility profile.

Regenerate with `node scripts/build-genos-compatibility.mjs <GENOS.idl>` from `src/`.
The generator requires the inspected source hash. Tests independently parse the
retained raw IDL fixture and compare every selectable address and compatibility
name against the combined embedded catalog.
