# Bundled Windows print helper

The print client prints on Windows by shelling out to **SumatraPDF**, a silent
command-line PDF printer. The executable is embedded into the Windows build at
compile time (`include_bytes!` in `src/print.rs`, gated behind
`#[cfg(windows)]`) and extracted to `%LOCALAPPDATA%\broccoli-print\` on first
use. macOS and Linux do not use this file and do not embed it.

## The vendored executable

`SumatraPDF.exe` here is the portable 64-bit build of SumatraPDF **3.5.2**,
fetched from the official site:
`https://www.sumatrapdfreader.org/dl/rel/3.5.2/SumatraPDF-3.5.2-64.zip`

SHA-256 (`SumatraPDF.exe`):
`290e4aa7ed64c728138711c011e89aab7aa48dbc1ae430371dc2be4100b92bf0`

To update it: download the new portable build, extract the `.exe`, save it here
as `SumatraPDF.exe`, replace the version and SHA-256 above, and bump
`SUMATRA_VERSION` in `src/print.rs` so the extracted copy on each station is
replaced.

## License

SumatraPDF is free software (GPLv3, with third-party components such as MuPDF
under their own licenses). See `SumatraPDF-LICENSE.txt`. Redistributing the
binary requires shipping the license and pointing to the corresponding source
for the exact version bundled, which the license file does.
