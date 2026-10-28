# Visibility probe (test-only)

Test-only scripted KWin effect that reads `EffectWindow.visible` for
`OmniTilerTestWindow` fixture windows only. It never renders, never writes
window properties, and never logs captions, titles, or foreign-window data.

This tree is durability housing for the effect plus its
25 mocked regression checks. Nothing here is auto-installed or included in
any runtime package (see "Packaging" below). As tracked source it does
travel in the release source archive built by
`scripts/build-source-archive.sh` (git archive of the tag, like the rest of
`kwin/tests/`); archive presence is not installation. Running the mocked
checks is safe and performs no live action - nothing here touches live KWin
state. Live loading of the effect itself is a separate step that requires
its own explicit authorization under the live KWin guide and the
fixture-only boundary.

## Layout

- `kwin/effects/omnitiler-visibility-probe/metadata.json` - KPackage
  metadata (`KWin/Effect`, `X-Plasma-API: javascript`,
  `EnabledByDefault: false`). `Id` matches the directory name.
- `kwin/effects/omnitiler-visibility-probe/contents/code/main.js` -
  the effect. Read-only: `stackingOrder`/`activeWindow` reads plus
  `visible`/`windowClass`/`geometry`/`pid` reads, one
  `omnitiler:visibility:` log line per fixture window. Join key per line:
  `{vis, pid, geom, active, trigger}` - no id field. Identical keys on
  sibling twins mean "unknown attribution": disambiguate (move/resize one
  window) before attributing `vis` to a caller id.
- `tests/run-tests.mjs` - offline mocked checks (node, no dependencies).
  Path from `tests/` to the effect (`../kwin/effects/...`) mirrors the
  original staging layout, so the runner needs no path edits.

## Offline invocation

From this directory:

- `node tests/run-tests.mjs` - runs the mocked checks.
- `node --check kwin/effects/omnitiler-visibility-probe/contents/code/main.js`
- `node --check tests/run-tests.mjs`
- `node -e "JSON.parse(require('fs').readFileSync('kwin/effects/omnitiler-visibility-probe/metadata.json','utf8)'); console.log('metadata ok')"`

## Packaging

Excluded from runtime and install packaging by existing exact-match rules
(no new exclusion was invented for this tree). It remains present as
tracked source in the release source archive:

- `scripts/build-kpackage.sh` packs only `metadata.json`,
  `contents/code/main.js`, `contents/config/main.xml`,
  `contents/ui/config.ui`, `LICENSE` from `kwin/` - none match this path.
- Arch (`packaging/arch/PKGBUILD`), Debian (`packaging/debian/rules`), and
  RPM (`packaging/rpm/omnitiler.spec`) install the same explicit four-file
  KWin script set - none match this path.
- `flake.nix` `kwinScriptSource` filesets only `kwin/metadata.json`,
  `kwin/package.json`, `kwin/package-lock.json`, `kwin/src`,
  `kwin/contents/config/main.xml`, `kwin/contents/ui/config.ui` -
  `kwin/tests/` is not included.
- `kwin/package.json` `test` bundles only `tests/*.test.ts`; the `.mjs`
  runner here is not picked up by the shipped test harness.
- `scripts/build-source-archive.sh` archives the exact tracked source at the
  tag, so this tree travels in the release tarball as source only; no
  install rule consumes it.
