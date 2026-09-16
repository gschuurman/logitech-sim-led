# Provenance

The `.wxs`/`.wxl` files and `Bitmaps/` in this directory are a trimmed copy
of the `WixUI_InstallDir` dialog set from
[wixtoolset/wix](https://github.com/wixtoolset/wix)'s
`src/ext/UI/wixlib/`, vendored here because referencing it as a normal
extension (`-ext WixToolset.UI.wixext` + `<UIRef Id="WixUI_InstallDir" />`)
fails with WIX0094 ("inaccessible due to its protection level") on WiX
v5.0.2 -- a confirmed upstream bug, not a mistake in this project's own
`.wxs` authoring; see
[wixtoolset/issues#7370](https://github.com/wixtoolset/issues/issues/7370)
(closed "not planned", same error against a different dialog set,
`WixUI_Mondo`). The reporter's own workaround, which this follows, was to
inline the dialog set's source instead of referencing the compiled
extension library.

Licensed under the Microsoft Reciprocal License (MS-RL) -- see the header
comment in each file, and
<https://github.com/wixtoolset/wix/blob/main/LICENSE.TXT> for the full
text. This differs from the rest of the project (MIT, see the repo root
`LICENSE`); that's expected for a vendored third-party dialog set, not an
error.

## What was trimmed from the upstream source, and why

- Only the plain `WixUI_InstallDir_X64` UI variant is kept (this package
  only ships an x64 build) -- upstream generates X86/X64/A64 variants via
  a `?foreach` preprocessor loop this copy doesn't reproduce.
- The "extended path validation" variant (`WixUI_InstallDir_ExtendedPathValidation_*`)
  is dropped: it needs a native custom-action DLL (`uica.dll`) built as
  part of the extension itself, which isn't available outside it. The
  plain variant's basic `CheckTargetPath` validation (rejects paths that
  aren't a valid absolute local/UNC path) is what's kept.
- `InvalidDirDlg.wxs` (only used by the extended-validation variant above)
  and `Common_Platform.wxi`/`Common_x64.wxs` (only needed for that same
  native DLL) are dropped along with it.
- The alternate wizard styles upstream also ships from the same directory
  -- `WixUI_Advanced`, `WixUI_FeatureTree`, `WixUI_Mondo`, `WixUI_Minimal`,
  and the dialogs only *they* use (`AdvancedWelcomeEulaDlg`,
  `WelcomeEulaDlg`, `CustomizeDlg`, `FeaturesDlg`, `InstallScopeDlg`,
  `SetupTypeDlg`) -- are dropped; this package only wants the
  `WixUI_InstallDir` sequence.

## Updating this

If a future WiX version fixes the WIX0094 bug, the cleaner path is
reverting `product.wxs` to use `-ext WixToolset.UI.wixext` +
`<UIRef Id="WixUI_InstallDir" />` again and deleting this directory
entirely, rather than keeping it updated by hand.
