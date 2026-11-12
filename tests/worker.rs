use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT_PROJECT: AtomicU64 = AtomicU64::new(0);
const WIFF: &str = "wiff_dir/run.wiff";
const TRANSITIONS: &str = "Name,Q1,Q3\nFrag,500,100\nPrecursor,500,\n";

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "echomskit_worker_test_{}_{}",
            std::process::id(),
            NEXT_PROJECT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::create_dir(path.join("wiff_dir")).unwrap();
        fs::write(
            path.join("param.txt"),
            "transition_list = 'transitions.csv'\nsample_duration = 6\n",
        )
        .unwrap();
        fs::write(path.join("transitions.csv"), TRANSITIONS).unwrap();
        Self(path)
    }

    fn with_run() -> Self {
        let project = Self::new();
        project.write_wiff(&Fixture::default());
        project
    }

    fn run(&self, argument: Option<&str>) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_EchoMSkit"));
        command
            .env("ECHOMSKIT_PROJECT_DIR", &self.0)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(argument) = argument {
            command.arg(argument);
        }
        let mut child = command.spawn().unwrap();
        let started = Instant::now();
        while child.try_wait().unwrap().is_none() {
            if started.elapsed() > Duration::from_secs(5) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("worker did not terminate for {argument:?}");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        child.wait_with_output().unwrap()
    }

    fn succeed(&self, argument: &str) -> String {
        let output = self.run(Some(argument));
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn fail(&self, argument: &str) -> String {
        let output = self.run(Some(argument));
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert!(!output.status.success(), "worker accepted invalid input");
        assert!(!stderr.contains("panicked"), "{stderr}");
        stderr
    }

    fn read(&self, file: &str) -> String {
        fs::read_to_string(self.0.join(file)).unwrap()
    }

    fn write(&self, file: &str, contents: &str) {
        fs::write(self.0.join(file), contents).unwrap();
    }

    fn write_wiff(&self, fixture: &Fixture) {
        fixture.write(&self.0.join(WIFF));
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

/* a two experiment echo run (TOF MS plus TOF MS/MS of 500) with one cycle per
second and 6 cycle ejections starting at the given cycles with the given tic,
by default at cycles 4..=9 and 16..=21. bin = 10000 *
sqrt(mz) with a = 0.001, t0 = 0 and a zero start bin */
struct Fixture {
    cycles: u32,
    wells: &'static str,
    second_sample: bool,
    plateaus: &'static [(u32, f64)],
    partial_last_cycle: bool,
}

impl Default for Fixture {
    fn default() -> Self {
        Self {
            cycles: 30,
            wells: "A1, A2",
            second_sample: false,
            plateaus: &[(4, 1000.0), (16, 1000.0)],
            partial_last_cycle: false,
        }
    }
}

fn utf16(text: &str) -> Vec<u8> {
    let units = text
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let mut bytes = (units.len() as u16).to_le_bytes().to_vec();
    bytes.extend(units);
    bytes
}

fn experiment_header(scan_type: u16, precursor: f64) -> Vec<u8> {
    let mut bytes = vec![0; 42];
    bytes.extend(precursor.to_le_bytes());
    bytes.resize(90, 0);
    bytes.extend(utf16("Exp"));
    bytes.extend([0, 0]);
    bytes.extend(scan_type.to_le_bytes());
    bytes.resize(bytes.len() + 16, 0);
    bytes
}

fn encode_bins(bins: &[(u32, u32)]) -> Vec<u8> {
    let mut stream = Vec::new();
    let mut at = 0;
    for &(bin, value) in bins {
        if bin > at {
            stream.push(0xfe);
            stream.extend((bin - at).to_le_bytes());
        }
        if value < 0x7c {
            stream.push(value as u8);
        } else {
            stream.push(0x7d);
            stream.extend((value as u16).to_le_bytes());
        }
        at = bin + 1;
    }
    stream.push(0xff);
    stream
}

impl Fixture {
    fn plateau(&self, cycle: u32) -> Option<f64> {
        self.plateaus
            .iter()
            .find(|(start, _)| (*start..start + 6).contains(&cycle))
            .map(|&(_, tic)| tic)
    }

    fn write(&self, wiff: &Path) {
        let mut index = vec![0_u8; 32];
        let mut data = Vec::new();
        for cycle in 0..self.cycles {
            let plateau = self.plateau(cycle);
            let lit = plateau.is_some();
            let experiments = if self.partial_last_cycle && cycle + 1 == self.cycles {
                1
            } else {
                2
            };
            for experiment in 0..experiments {
                let bins: &[(u32, u32)] = match (lit, experiment) {
                    (false, _) => &[],
                    (true, 0) => &[(223_606, 1), (223_607, 2), (223_608, 1)],
                    (true, _) => &[(99_999, 5), (100_000, 10), (100_001, 5)],
                };
                let mut header = vec![0x0a, 18, 0x09];
                header.extend(0.001_f64.to_le_bytes());
                header.push(0x11);
                header.extend(0.0_f64.to_le_bytes());
                data.push(header.len() as u8);
                data.extend(header);

                let marker = data.len();
                data.extend([0xff; 4]);
                data.extend(0_u32.to_le_bytes());
                data.extend(encode_bins(bins));
                while (data.len() - marker) % 4 != 0 {
                    data.push(0xff);
                }

                let time = f64::from(cycle * 1000 + experiment * 100);
                let tic = plateau.unwrap_or(10.0);
                index.extend((marker as u32).to_le_bytes());
                index.extend(((data.len() - marker) as u32).to_le_bytes());
                index.extend(time.to_le_bytes());
                index.extend(6_u16.to_le_bytes());
                index.extend(tic.to_le_bytes());
                index.extend([0; 28]);
            }
        }

        let mut scan = 2200_u32.to_le_bytes().to_vec();
        scan.extend(((20 + 24 + data.len()) as u64).to_le_bytes());
        scan.resize(20, 0);
        scan.extend(0x1111_1111_u32.to_le_bytes());
        scan.extend(2200_u32.to_le_bytes());
        scan.extend(1_u32.to_le_bytes());
        scan.extend(((24 + data.len()) as u64).to_le_bytes());
        scan.extend(0_u32.to_le_bytes());
        scan.extend(data);
        let mut scan_path = wiff.as_os_str().to_owned();
        scan_path.push(".scan");
        fs::write(scan_path, scan).unwrap();

        let mut dabs = vec![0_u8; 32];
        dabs.extend(2_u32.to_le_bytes());
        dabs.extend(utf16("run.wiff2"));
        dabs.extend(0_u32.to_le_bytes());
        dabs.extend(utf16(self.wells));

        let method = "/MethodSubtree/Method1/DeviceMethod0/Period0";
        let mut file = cfb::create(wiff).unwrap();
        let mut put = |path: &str, bytes: &[u8]| {
            let parent = path.rsplit_once('/').unwrap().0;
            file.create_storage_all(parent).unwrap();
            file.create_stream(path).unwrap().write_all(bytes).unwrap();
        };
        put(
            &format!("{method}/Experiment0/ExperimentHeader"),
            &experiment_header(8, 0.0),
        );
        put(
            &format!("{method}/Experiment1/ExperimentHeader"),
            &experiment_header(9, 500.0),
        );
        put("/SampleSubtree/Sample1/Idx", &index);
        put("/SampleSubtree/DabsInfo", &dabs);
        if self.second_sample {
            put("/SampleSubtree/Sample2/Idx", &index[..32]);
        }
        file.flush().unwrap();
    }
}

#[test]
fn detected_windows_feed_area_calculation() {
    let project = Project::with_run();
    project.succeed("1");
    let acquisition = project.read("acq_time.csv");
    assert_eq!(
        acquisition,
        "# sample_duration = 6\nrun.wiff,,\n1 A1,0.043,0.176\n2 A2,0.242,0.376\n"
    );
    project.succeed("2only");
    let long = project.read("long.csv");
    // six points of 20 one second apart plus both ramps from 0, trapezoids in intensity seconds
    assert!(
        long.contains("run.wiff,Frag,1 A1,0.043,0.176,0.152,120.0,20.0"),
        "{long}"
    );
    assert!(
        long.contains("run.wiff,Precursor,1 A1,0.043,0.176,0.150,24.0,4.0"),
        "{long}"
    );
    assert!(long.contains("run.wiff,Frag,2 A2,0.242,0.376,"), "{long}");
    assert_eq!(long.lines().count(), 5);

    let plot = fs::read(project.0.join("misc/plot_run.wiff.bin")).unwrap();
    assert_eq!(&plot[..2], &2_u16.to_le_bytes());
    assert_eq!(&plot[2..7], b"Frag\0");
    assert_eq!(&plot[7..11], &30_u32.to_le_bytes());
}

#[test]
fn rerun_keeps_edited_windows_until_duration_changes() {
    let project = Project::with_run();
    let edited = "# sample_duration = 6\nrun.wiff,,\n1 A1,0.050,0.170\n";
    project.write("acq_time.csv", edited);
    project.succeed("1");
    assert_eq!(project.read("acq_time.csv"), edited);

    project.write(
        "param.txt",
        "transition_list = 'transitions.csv'\nsample_duration = 3\n",
    );
    project.succeed("1");
    let acquisition = project.read("acq_time.csv");
    assert!(
        acquisition.starts_with("# sample_duration = 3\n"),
        "{acquisition}"
    );
    // each 6 s plateau now holds two 3 s samples
    assert_eq!(acquisition.lines().count(), 6, "{acquisition}");
}

#[test]
fn well_list_mismatch_numbers_windows() {
    let project = Project::new();
    project.write_wiff(&Fixture {
        wells: "A1",
        ..Fixture::default()
    });
    let stdout = project.succeed("1");
    assert!(stdout.contains("numbering them instead"), "{stdout}");
    assert!(
        project
            .read("acq_time.csv")
            .contains("\n1,0.043,0.176\n2,0.242")
    );
}

#[test]
fn sciex_style_transition_sheet_loads_by_header() {
    let project = Project::with_run();
    project.write(
        "transitions.csv",
        "No.,MS_Lvl,Group,Name,Adduct/Charge,Precursor (Q1) Mass (Da),Fragment (Q3) Mass (Da),XIC Width (Da)\n\
         1,2,X,Frag,[M+H]+,500.1,100,0.05\n2,1,X,Precursor,[M+H]+,500,,0.02\n",
    );
    project.succeed("1");
    project.succeed("2only");
    let long = project.read("long.csv");
    assert!(
        long.contains("run.wiff,Frag,1 A1,0.043,0.176,0.152,120.0,20.0"),
        "{long}"
    );
    assert!(
        long.contains("run.wiff,Precursor,1 A1,0.043,0.176,0.150,24.0,4.0"),
        "{long}"
    );
}

#[test]
fn unmatched_precursor_names_the_transition() {
    let project = Project::with_run();
    project.succeed("1");
    project.write(
        "transitions.csv",
        "Name,Q1,Q3\nFrag,500,100\nMissing,700,100\n",
    );
    fs::create_dir(project.0.join("misc")).unwrap();
    let plot = project.0.join("misc/plot_run.wiff.bin");
    fs::write(&plot, b"previous valid plot").unwrap();
    let stderr = project.fail("2only");
    assert!(stderr.contains("Missing"), "{stderr}");
    assert_eq!(fs::read(&plot).unwrap(), b"previous valid plot");
    assert!(!project.0.join("long.csv").exists());
}

#[test]
fn damaged_scan_data_fails_without_results() {
    let damage: [fn(&mut Vec<u8>); 4] = [
        |scan| scan.truncate(scan.len() - 40),
        |scan| scan[20] = 0,
        |scan| scan[44] = 0x40,
        |scan| {
            let at = scan.len() - 4;
            scan[at..].fill(0x7f);
        },
    ];
    for damage in damage {
        let project = Project::with_run();
        project.succeed("1");
        let path = project.0.join(format!("{WIFF}.scan"));
        let mut scan = fs::read(&path).unwrap();
        damage(&mut scan);
        fs::write(&path, scan).unwrap();
        project.fail("2only");
        assert!(!project.0.join("long.csv").exists());
    }
}

#[test]
fn unsupported_or_missing_data_reports_errors() {
    let project = Project::new();
    project.write_wiff(&Fixture {
        second_sample: true,
        ..Fixture::default()
    });
    assert!(project.fail("1").contains("more than one sample"));

    let project = Project::new();
    assert!(project.fail("1").contains("no .wiff files"));

    let project = Project::new();
    fs::write(project.0.join(WIFF), b"not a compound file").unwrap();
    assert!(project.fail("1").contains("run.wiff"));
    assert!(!project.0.join("acq_time.csv").exists());
}

#[test]
fn malformed_param_returns_error_without_panic() {
    for param in [
        "transition_list = 'transitions.csv'\n",
        "transition_list = 'transitions.csv'\nsample_duration = 0\n",
        "transition_list = 'transitions.csv'\nsample_duration = 6\nmass_tolerance = -1\n",
        "sample_duration = 6\n",
    ] {
        let project = Project::with_run();
        project.write("param.txt", param);
        project.fail("1");
    }
}

#[test]
fn malformed_acquisition_csv_returns_error_without_panic() {
    for csv in [
        "run.wiff\n",
        "run.wiff,,\nwell,invalid,2\n",
        "well,0,2\nrun.wiff,,\nwell,0,2\n",
        "run.wiff,,\nwell,2,0\n",
        "run.wiff,,\nwell,NaN,2\n",
        "missing.wiff,,\nwell,0,2\n",
        "",
    ] {
        let project = Project::with_run();
        project.write("acq_time.csv", csv);
        project.fail("2only");
        assert!(!project.0.join("long.csv").exists(), "{csv}");
    }
}

#[test]
fn malformed_assay_csv_returns_error_without_panic() {
    for csv in [
        "Name,Q1,Q3\nTest,invalid,50\n",
        "Name,Q1,Q3\nTest,NaN,50\n",
        "Name,Q1,Q3\nTest,,50\n",
        "Name,Q1,Q3\n,500,100\n",
        "Name,Q1,Q3,Width\nTest,500,100,0\n",
        "Name,Q1,Q3\n",
    ] {
        let project = Project::with_run();
        project.succeed("1");
        project.write("transitions.csv", csv);
        project.fail("2only");
        assert!(!project.0.join("long.csv").exists(), "{csv}");
    }
}

#[test]
fn interactive_worker_exits_when_stdin_closes() {
    let project = Project::new();
    assert!(project.run(None).status.success());
}

#[test]
fn weak_ejection_and_partial_last_cycle_still_detected() {
    let project = Project::new();
    project.write_wiff(&Fixture {
        plateaus: &[(4, 1000.0), (16, 300.0)],
        partial_last_cycle: true,
        ..Fixture::default()
    });
    project.succeed("1");
    assert_eq!(
        project.read("acq_time.csv"),
        "# sample_duration = 6\nrun.wiff,,\n1 A1,0.043,0.176\n2 A2,0.243,0.376\n"
    );
    project.succeed("2only");
    assert!(project.read("long.csv").contains("run.wiff,Frag,2 A2,"));
}

#[test]
fn missing_scan_data_or_lone_wiff2_is_explained() {
    let project = Project::with_run();
    fs::remove_file(project.0.join(format!("{WIFF}.scan"))).unwrap();
    let stdout = project.succeed("1");
    assert!(stdout.contains("has no .wiff.scan"), "{stdout}");
    let stderr = project.fail("2only");
    assert!(
        stderr.contains(".wiff.scan file next to the .wiff"),
        "{stderr}"
    );
    assert!(!project.0.join("long.csv").exists());

    let project = Project::with_run();
    fs::write(project.0.join("wiff_dir/RUN.WIFF2"), b"").unwrap();
    fs::write(project.0.join("wiff_dir/other.wiff2"), b"").unwrap();
    let stderr = project.fail("1");
    assert!(
        stderr.contains("other.wiff2 has no matching .wiff"),
        "{stderr}"
    );
    assert!(!stderr.contains("RUN.WIFF2"), "{stderr}");
}

#[test]
fn blank_ejections_are_placed_from_the_spacing() {
    let windows = |cycles, plateaus, wells| {
        let project = Project::new();
        project.write_wiff(&Fixture {
            cycles,
            plateaus,
            wells,
            ..Fixture::default()
        });
        let stdout = project.succeed("1");
        (stdout, project.read("acq_time.csv"))
    };

    // the third of four wells is blank, its window lands between its neighbours
    let (stdout, acquisition) = windows(
        52,
        &[(4, 1000.0), (16, 1000.0), (40, 1000.0)],
        "A1, A2, A3, A4",
    );
    assert!(stdout.contains("no signal for 3 A3"), "{stdout}");
    assert!(
        acquisition
            .ends_with("1 A1,0.043,0.176\n2 A2,0.242,0.376\n3 A3,0.442,0.576\n4 A4,0.642,0.776\n"),
        "{acquisition}"
    );

    // a blank last well cannot be told from dead time at the end, so they get numbered
    let (stdout, acquisition) = windows(
        48,
        &[(4, 1000.0), (16, 1000.0), (28, 1000.0)],
        "A1, A2, A3, A4",
    );
    assert!(stdout.contains("numbering them instead"), "{stdout}");
    assert!(acquisition.ends_with("\n3,0.442,0.576\n"), "{acquisition}");

    // a stray plateau makes up for the blank, the uneven spacing is flagged
    let (stdout, acquisition) = windows(
        60,
        &[(4, 1000.0), (16, 1000.0), (40, 1000.0), (47, 1000.0)],
        "A1, A2, A3, A4",
    );
    assert!(stdout.contains("unevenly spaced"), "{stdout}");
    assert!(acquisition.contains("\n4 A4,"), "{acquisition}");
}

#[test]
fn mass_tolerance_overrides_transition_widths() {
    let project = Project::with_run();
    // the fragment sits at 100.00, 0.05 Da off this q3
    project.write(
        "transitions.csv",
        "Name,Q1,Q3,XIC Width (Da)\nFrag,500,100.05,0.02\n",
    );
    project.succeed("1");
    project.succeed("2only");
    assert!(
        project
            .read("long.csv")
            .contains("run.wiff,Frag,1 A1,0.043,0.176,0.185,0.0,0.0"),
        "{}",
        project.read("long.csv")
    );

    project.write(
        "param.txt",
        "transition_list = 'transitions.csv'\nsample_duration = 6\nmass_tolerance = 0.06\n",
    );
    project.succeed("2only");
    let long = project.read("long.csv");
    assert!(
        long.contains("run.wiff,Frag,1 A1,0.043,0.176,0.152,120.0,20.0"),
        "{long}"
    );
}
