# EchoMSkit

EchoMSkit processes targeted (MRMHR) data from SCIEX Echo MS experiments. It is free and open source under the BSD 3-Clause license, see [LICENSE](LICENSE).

## What it does

The tool takes the following as input: (i) SCIEX `.wiff` files with their `.wiff.scan` data, one per acquisition, (ii) a transition list with Q1 and Q3 masses and (iii) the approximate run time of each sample. The scans are read directly from the SCIEX files, so no vendor software or mzML conversion is needed. The package finds each ejection in the total ion trace, extracts the transition chromatograms from the TOF MS and TOF MS/MS scans, and reports the ion intensities integrated over each sample window.

## Project folder

```
param.txt          settings, see below
transitions.csv    transition list
sample_list.csv    optional sample ids for the results table
wiff_dir/          acquisitions, each name.wiff next to name.wiff.scan
```

The GUI sets up a raw SCIEX export when the folder is selected or refreshed: `.wiff`, `.wiff.scan` and `.wiff2` files are moved into `wiff_dir/`, a transition workbook (`.xlsx`, `.xlsm` or `.xls`, named like a transition list or the only one in the folder) is converted to a CSV of the same name, and a missing `param.txt` is written pointing at that CSV. `sample_duration` is read from a seconds token in the acquisition names (`..._30s_...`), otherwise it defaults to 30. Workbooks often hold the acquisition method next to the transitions, so the sheet whose header has both Q1 and Q3 columns is converted. Existing files are never overwritten, and every change is listed in the activity log.

`param.txt` is TOML:

```toml
transition_list = "transitions.csv"
sample_duration = 30   # seconds of signal per ejection, the window width
mass_tolerance = 0.03  # optional, ± Da, applied to every transition
```

The transition list needs a name, Q1 and Q3 column. Columns are matched by header, so SCIEX style sheets (`Name`, `Precursor (Q1) Mass (Da)`, `Fragment (Q3) Mass (Da)`, `XIC Width (Da)`) and plain `Name,Q1,Q3` lists both work. A blank Q3 traces the precursor in the TOF MS scan; otherwise the Q3 mass is extracted from the TOF MS/MS experiment whose precursor is within 0.5 Da of Q1.

Each chromatogram sums the signal within a mass tolerance of the target. Without `mass_tolerance` in `param.txt`, each transition uses half its `XIC Width` as the tolerance (SCIEX sheets give the full width), or ± 0.01 Da when it has none. Setting `mass_tolerance` applies one tolerance to every transition, like RFkit, which is the quickest way to loosen or tighten the extraction for the whole list.

The sample list has `File`, `Index` and `Sample_ID` columns. `Index` is the ejection number within an acquisition, and a blank `File` applies the row to every acquisition.

## Outputs

- `acq_time.csv` lists the detected window of every ejection, labelled with its number and the Echo well. Each window is `sample_duration` plus one scan cycle on either side, so the rising edge of late-cycle transitions stays inside; back to back windows meet halfway instead of overlapping. Edited bounds are kept on later runs unless `sample_duration` changes.
- `long.csv` (command line) or `EchoMSkit_results.csv` (GUI) has the window start and end, apex time, area and height per transition and sample. Times are in minutes.
- `misc/` holds the chromatogram data used by the GUI visualizer and the PDF plots.

## Command line

```bash
EchoMSkit 1        # find sample windows, write acq_time.csv
EchoMSkit 2        # calculate areas, write long.csv and PDF chromatograms (needs R)
EchoMSkit 2only    # calculate areas only
```

Run without arguments for the interactive menu. The project folder defaults to the executable's folder; set `ECHOMSKIT_PROJECT_DIR` to use another one. `ECHOMSKIT_RSCRIPT` and `ECHOMSKIT_PLOT_SCRIPT` point at Rscript and `EchoMSkit_plot.r` when they are not found automatically.

## Building from source

Once the repository is downloaded:

```bash
cd EchoMSkit-GUI
cargo build --release
```

Windows:
```powershell
cd EchoMSkit-GUI
powershell -ExecutionPolicy Bypass -File app/build-windows.ps1
```

The script creates a lightweight per-user setup executable under
`app/src-tauri/target/release/bundle/nsis`. It bundles the EchoMSkit processing
engine, so the installed GUI does not require Rust, R, or any source files.
The setup program uses the WebView2 runtime already present on current Windows
systems. If WebView2 is missing, setup downloads Microsoft's small bootstrapper
at that point instead of making every download include the full runtime.

MacOS:
```bash
cd EchoMSkit-GUI
bash app/build-macos.sh
```

The default command creates a native, ad-hoc signed app and DMG for local
testing. The finished files are under `app/src-tauri/target/release/bundle`.

For a public macOS release, install a **Developer ID Application** certificate
and its private key in the login keychain. Then run:

```bash
bash app/build-macos.sh --release --universal
```

To use a `notarytool` Keychain profile instead of environment variables:

```bash
bash app/build-macos.sh --release --universal --notary-profile echomskit-notary
```

The release build is signed with the Developer ID identity, uses the hardened
runtime, is notarized, stapled, and checked with Gatekeeper. The universal
option creates one DMG that runs natively on both Apple Silicon and Intel Macs.

## Tests

Run from the repository root with Rust and Node.js 18 or newer installed:

```bash
cargo test
cargo test --locked --manifest-path app/src-tauri/Cargo.toml
node --test app/tests/frontend.test.cjs
cargo clippy --all-targets -- -D warnings
cargo clippy --locked --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings
cargo fmt --check
cargo fmt --check --manifest-path app/src-tauri/Cargo.toml
node --check app/ui/app.js
```

The GUI Rust checks require the platform's Tauri build prerequisites and worker
sidecar (prepared by the platform build script in `app/`). The worker tests build
small synthetic `.wiff` and `.wiff.scan` files and invoke the real CLI; they do
not modify existing datasets.

## Data format notes

SCIEX OS writes `.wiff2` as an encrypted database, so EchoMSkit reads the `.wiff`
and `.wiff.scan` pair that is written alongside it. A `.wiff2` without its `.wiff`
stops the run with a message, and a `.wiff` without its `.wiff.scan` still gets
its sample windows but cannot be integrated. Ejections are found where the total
ion trace rises a quarter of the way from its baseline to its plateau level. A
blank well never lifts the trace, so when fewer windows than Echo wells are
found, blanks between ejections are placed at the regular ejection spacing and
reported. A blank first or last well, uneven spacing or too many blanks leave
the windows numbered instead of labelled with wells, and a well count that
matches despite uneven spacing is flagged so the labels can be checked. Files holding more than one
sample, or methods with more than one period, are reported as unsupported rather
than guessed at. Centroiding follows the vendor peak list closely: on the
reference dataset the extracted ion totals have a median difference of 0.3% from
SCIEX's own centroided export.
