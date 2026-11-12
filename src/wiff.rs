/* SCIEX writes each acquisition as a .wiff (OLE container with the method, scan
index and sample info) plus a .wiff.scan holding the spectra. the .wiff2 that
SCIEX OS also writes is an encrypted database, so everything here comes from
the two unencrypted files. layouts were worked out against vendor-converted
mzML of ZenoTOF 7600 Echo MS runs */

use std::error::Error;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const METHOD: &str = "/MethodSubtree/Method1/DeviceMethod0";
const SAMPLE: &str = "/SampleSubtree/Sample1";
// every stream in the .wiff starts with the same 32 byte record header
const STREAM_HEADER: usize = 32;
const INDEX_ENTRY: usize = 54;
const SCAN_FILE_HEADER: u64 = 20;
const SCAN_BLOCK_MAGIC: u32 = 0x1111_1111;
/* centroiding: nonzero bins at most CLUSTER_GAP apart form a run, runs split
where the 3 point smoothed profile dips below VALLEY of the smaller peak on
either side, and single bin peaks (lone ion counts) are dropped. tuned on the
sample data, xic totals land within a few percent of the vendor peak list */
const CLUSTER_GAP: u32 = 4;
const VALLEY: f64 = 0.8;

pub struct Experiment {
    // 0 for scan types EchoMSkit does not extract from
    pub ms_level: u8,
    pub precursor: f64,
}

pub struct ScanEntry {
    offset: u64,
    size: u32,
    pub time: f32,
    pub tic: f64,
}

pub struct Acquisition {
    pub experiments: Vec<Experiment>,
    pub scans: Vec<ScanEntry>,
    pub wells: Vec<String>,
}

// (time in minutes, intensity) per scan of the target's experiment
pub type Xic = Vec<(f32, f32)>;

pub struct Target {
    pub experiment: usize,
    pub mz: f64,
    pub half_width: f64,
}

pub fn scan_path(wiff: &Path) -> PathBuf {
    let mut path = wiff.as_os_str().to_owned();
    path.push(".scan");
    PathBuf::from(path)
}

fn le<const N: usize>(bytes: &[u8], at: usize) -> Option<[u8; N]> {
    bytes.get(at..at + N)?.try_into().ok()
}

fn read_stream<F: Read + Seek>(
    file: &mut cfb::CompoundFile<F>,
    path: &str,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = Vec::new();
    file.open_stream(path)
        .map_err(|error| format!("{path}: {error}"))?
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

pub fn read_acquisition(wiff: &Path) -> Result<Acquisition, Box<dyn Error>> {
    let mut file = cfb::open(wiff)?;
    if file.exists("/SampleSubtree/Sample2") {
        return Err("files with more than one sample are not supported yet".into());
    }
    if file.exists(format!("{METHOD}/Period1")) {
        return Err("acquisition methods with more than one period are not supported".into());
    }

    let mut experiments = Vec::new();
    loop {
        let path = format!(
            "{METHOD}/Period0/Experiment{}/ExperimentHeader",
            experiments.len()
        );
        if !file.is_stream(&path) {
            break;
        }
        let bytes = read_stream(&mut file, &path)?;
        experiments.push(parse_experiment(&bytes).ok_or("experiment header is truncated")?);
    }
    if experiments.is_empty() {
        return Err("the acquisition method contains no experiments".into());
    }

    let index = read_stream(&mut file, &format!("{SAMPLE}/Idx"))?;
    let entries = index
        .get(STREAM_HEADER..)
        .filter(|entries| entries.len() % INDEX_ENTRY == 0)
        .ok_or("scan index has an unexpected size")?;
    let scans = entries
        .chunks_exact(INDEX_ENTRY)
        .map(|entry| ScanEntry {
            offset: u64::from(u32::from_le_bytes(le(entry, 0).unwrap())),
            size: u32::from_le_bytes(le(entry, 4).unwrap()),
            time: (f64::from_le_bytes(le(entry, 8).unwrap()) / 60_000.0) as f32,
            tic: f64::from_le_bytes(le(entry, 18).unwrap()),
        })
        .collect::<Vec<_>>();
    if scans.is_empty() {
        return Err("the acquisition contains no scans".into());
    }
    if scans.windows(2).any(|pair| pair[1].time < pair[0].time) {
        return Err("scan times are not in acquisition order".into());
    }

    // the echo well list is optional, windows fall back to numbered labels
    let wells = read_stream(&mut file, "/SampleSubtree/DabsInfo")
        .ok()
        .and_then(|bytes| parse_wells(&bytes))
        .unwrap_or_default();

    Ok(Acquisition {
        experiments,
        scans,
        wells,
    })
}

// precursor at 42, then the experiment name and a u16 field before the scan type
fn parse_experiment(bytes: &[u8]) -> Option<Experiment> {
    let precursor = f64::from_le_bytes(le(bytes, 42)?);
    let name_len = usize::from(u16::from_le_bytes(le(bytes, 90)?));
    let ms_level = match u16::from_le_bytes(le(bytes, 94 + name_len)?) {
        8 => 1,
        9 => 2,
        _ => 0,
    };
    Some(Experiment {
        ms_level,
        precursor,
    })
}

// utf-16 strings in the .wiff carry a u16 byte length prefix
fn utf16_at(bytes: &[u8], at: usize) -> Option<(String, usize)> {
    let len = usize::from(u16::from_le_bytes(le(bytes, at)?));
    let text = bytes.get(at + 2..at + 2 + len)?;
    let units = text
        .chunks_exact(2)
        .map(|unit| u16::from_le_bytes([unit[0], unit[1]]));
    Some((
        char::decode_utf16(units).collect::<Result<_, _>>().ok()?,
        at + 2 + len,
    ))
}

// DabsInfo holds the data file name, a u32, then the ejected wells as "A1, A2, ..."
fn parse_wells(bytes: &[u8]) -> Option<Vec<String>> {
    let (_, next) = utf16_at(bytes, STREAM_HEADER + 4)?;
    let (wells, _) = utf16_at(bytes, next + 4)?;
    let wells = wells
        .split(',')
        .map(|well| well.trim().to_string())
        .filter(|well| !well.is_empty())
        .collect::<Vec<_>>();
    (!wells.is_empty()).then_some(wells)
}

struct Calibration {
    a: f64,
    t0: f64,
}

fn varint(bytes: &[u8], at: &mut usize) -> Option<u64> {
    let mut value = 0_u64;
    for shift in (0..64).step_by(7) {
        let byte = *bytes.get(*at)?;
        *at += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte < 0x80 {
            return Some(value);
        }
    }
    None
}

// pulls field 1 {1: a, 2: t0} out of the protobuf scan header
fn parse_calibration(bytes: &[u8]) -> Option<Calibration> {
    fn fields(bytes: &[u8], mut visit: impl FnMut(u64, &[u8])) -> Option<()> {
        let mut at = 0;
        while at < bytes.len() {
            let key = varint(bytes, &mut at)?;
            let len = match key & 7 {
                0 => {
                    varint(bytes, &mut at)?;
                    0
                }
                1 => 8,
                2 => usize::try_from(varint(bytes, &mut at)?).ok()?,
                5 => 4,
                _ => return None,
            };
            visit(key >> 3, bytes.get(at..at + len)?);
            at += len;
        }
        Some(())
    }

    let mut calibration = None;
    fields(bytes, |field, value| {
        if field == 1 && calibration.is_none() {
            let mut a = None;
            let mut t0 = None;
            fields(value, |field, value| {
                let number = le(value, 0).map(f64::from_le_bytes);
                match field {
                    1 => a = number,
                    2 => t0 = number,
                    _ => {}
                }
            });
            calibration = Some((a, t0));
        }
    })?;
    let (Some(a), Some(t0)) = calibration? else {
        return None;
    };
    (a.is_finite() && a > 0.0 && t0.is_finite()).then_some(Calibration { a, t0 })
}

// walks the run-length coded tof bins and reports each nonzero intensity
fn decode_bins(
    stream: &[u8],
    start: u32,
    mut emit: impl FnMut(u32, u32),
) -> Result<(), &'static str> {
    const TRUNCATED: &str = "scan data ended unexpectedly";
    // little endian operand following an op byte
    fn operand(stream: &[u8], at: &mut usize, len: usize) -> Result<u32, &'static str> {
        let bytes = stream.get(*at + 1..*at + 1 + len).ok_or(TRUNCATED)?;
        *at += 1 + len;
        Ok(bytes
            .iter()
            .rev()
            .fold(0, |value, &byte| (value << 8) | u32::from(byte)))
    }

    let mut bin = start;
    let mut at = 0;
    loop {
        let op = *stream.get(at).ok_or(TRUNCATED)?;
        let (value, skip) = match op {
            0x00..=0x7b => {
                at += 1;
                (u32::from(op), 1)
            }
            0x7c => (operand(stream, &mut at, 1)?, 1),
            0x7d => (operand(stream, &mut at, 2)?, 1),
            0x7e => (operand(stream, &mut at, 4)?, 1),
            0x80..=0xfb => {
                at += 1;
                (0, u32::from(op - 0x80))
            }
            0xfc => (0, operand(stream, &mut at, 1)?),
            0xfd => (0, operand(stream, &mut at, 2)?),
            0xfe => (0, operand(stream, &mut at, 4)?),
            0xff => return Ok(()),
            0x7f => return Err("scan data uses an unknown encoding"),
        };
        if value > 0 {
            emit(bin, value);
        }
        bin = bin.checked_add(skip).ok_or("scan data is out of range")?;
    }
}

// reports (summed intensity, intensity weighted bin) for each peak in a run
fn centroids(run: &[(u32, u32)], profile: &mut Vec<(f64, f64)>, mut emit: impl FnMut(f64, f64)) {
    let n = run.len();
    if n < 2 {
        return;
    }
    let value = |at: usize| f64::from(run[at].1);
    // smoothed intensity and the highest smoothed value from here to the end
    profile.clear();
    profile.extend((0..n).map(|at| {
        let smoothed = (value(at.saturating_sub(1)) + value(at) + value((at + 1).min(n - 1))) / 3.0;
        (smoothed, smoothed)
    }));
    for at in (0..n - 1).rev() {
        profile[at].1 = profile[at].1.max(profile[at + 1].1);
    }

    let mut lo = 0;
    let mut left_max = 0.0_f64;
    for at in 0..n {
        let (smoothed, right_max) = profile[at];
        left_max = left_max.max(smoothed);
        let valley = at > 0
            && at + 1 < n
            && smoothed < profile[at - 1].0
            && smoothed <= profile[at + 1].0
            && smoothed < VALLEY * left_max.min(right_max);
        if !valley && at + 1 < n {
            continue;
        }
        if at > lo {
            let (weight, weighted_bin) =
                run[lo..=at]
                    .iter()
                    .fold((0.0, 0.0), |(weight, weighted), &(bin, value)| {
                        let value = f64::from(value);
                        (weight + value, weighted + f64::from(bin) * value)
                    });
            emit(weight, weighted_bin);
        }
        lo = at + 1;
        left_max = 0.0;
    }
}

struct Window {
    lo: f64,
    hi: f64,
    target: usize,
}

/* sums centroid intensities inside each target window for every scan of the
targeted experiments. scans are read in file order with one reusable buffer,
and experiments without targets are skipped without decoding */
pub fn extract_xics(
    wiff: &Path,
    acquisition: &Acquisition,
    targets: &[Target],
) -> Result<Vec<Xic>, Box<dyn Error>> {
    let experiments = acquisition.experiments.len();
    let mut by_experiment: Vec<Vec<usize>> = vec![Vec::new(); experiments];
    for (index, target) in targets.iter().enumerate() {
        by_experiment[target.experiment].push(index);
    }
    let cycles = acquisition.scans.len().div_ceil(experiments);
    let mut xics = vec![Vec::with_capacity(cycles); targets.len()];

    let scan = File::open(scan_path(wiff)).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => {
            "the .wiff.scan file next to the .wiff was not found, it holds the scan data"
                .to_string()
        }
        _ => format!(".wiff.scan could not be opened: {error}"),
    })?;
    let mut reader = BufReader::with_capacity(1 << 20, scan);
    reader.seek(SeekFrom::Start(SCAN_FILE_HEADER))?;
    let mut block = [0_u8; 24];
    reader.read_exact(&mut block)?;
    if u32::from_le_bytes(le(&block, 0).unwrap()) != SCAN_BLOCK_MAGIC
        || u32::from_le_bytes(le(&block, 8).unwrap()) != 1
    {
        return Err("the .wiff.scan file does not start with sample 1".into());
    }
    let data_start = SCAN_FILE_HEADER + 24;

    let mut cursor = data_start;
    let mut record = Vec::new();
    let mut windows = Vec::new();
    let mut sums = Vec::new();
    let mut run = Vec::new();
    let mut profile = Vec::new();
    for (scan_index, scan) in acquisition.scans.iter().enumerate() {
        let marker = data_start + scan.offset;
        let header_len = marker
            .checked_sub(cursor)
            .filter(|len| (1..1 << 16).contains(len))
            .ok_or("scan index does not match the .wiff.scan file")?;
        let wanted = &by_experiment[scan_index % experiments];
        cursor = marker + u64::from(scan.size);
        if wanted.is_empty() {
            reader.seek_relative((header_len + u64::from(scan.size)) as i64)?;
            continue;
        }

        record.resize((header_len + u64::from(scan.size)) as usize, 0);
        reader.read_exact(&mut record)?;
        let (header, data) = record.split_at(header_len as usize);
        let mut at = 0;
        let calibration = varint(header, &mut at)
            .filter(|&len| at as u64 + len == header_len)
            .and_then(|_| parse_calibration(&header[at..]))
            .ok_or("scan header is malformed")?;
        if data.get(..4) != Some(&[0xff; 4]) {
            return Err("scan record marker is missing".into());
        }
        let start = u32::from_le_bytes(le(data, 4).ok_or("scan record is truncated")?);

        // sqrt(mz) = a * ((bin - 0.75 * start) / 10 - t0)
        let bin_of =
            |mz: f64| 0.75 * f64::from(start) + 10.0 * (mz.sqrt() / calibration.a + calibration.t0);
        windows.clear();
        windows.extend(wanted.iter().map(|&target| {
            let Target { mz, half_width, .. } = targets[target];
            Window {
                lo: bin_of(mz - half_width),
                hi: bin_of(mz + half_width),
                target,
            }
        }));
        windows.sort_unstable_by(|left, right| left.lo.total_cmp(&right.lo));
        let max_span = windows
            .iter()
            .map(|window| window.hi - window.lo)
            .fold(0.0, f64::max);
        sums.clear();
        sums.resize(windows.len(), 0.0_f64);

        let mut add_cluster = |weight: f64, weighted_bin: f64| {
            let centroid = weighted_bin / weight;
            let end = windows.partition_point(|window| window.lo <= centroid);
            for (window, sum) in windows[..end].iter().zip(&mut sums[..end]).rev() {
                if window.lo < centroid - max_span {
                    break;
                }
                if centroid <= window.hi {
                    *sum += weight;
                }
            }
        };
        run.clear();
        decode_bins(&data[8..], start, |bin, value| {
            if run
                .last()
                .is_some_and(|&(last, _)| bin - last > CLUSTER_GAP)
            {
                centroids(&run, &mut profile, &mut add_cluster);
                run.clear();
            }
            run.push((bin, value));
        })?;
        centroids(&run, &mut profile, &mut add_cluster);

        for (window, sum) in windows.iter().zip(&sums) {
            xics[window.target].push((scan.time, *sum as f32));
        }
    }
    Ok(xics)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_every_value_and_skip_width() {
        let stream = [
            0x05, 0x7c, 0xc8, 0x7d, 0x10, 0x27, 0x7e, 0xa0, 0x86, 0x01, 0x00, 0x83, 0x02, 0xfc,
            0x0a, 0x03, 0xfd, 0x00, 0x01, 0x04, 0xfe, 0x00, 0x00, 0x01, 0x00, 0x00, 0x06, 0xff,
        ];
        let mut bins = Vec::new();
        decode_bins(&stream, 100, |bin, value| bins.push((bin, value))).unwrap();
        assert_eq!(
            bins,
            [
                (100, 5),
                (101, 200),
                (102, 10_000),
                (103, 100_000),
                (107, 2),
                (118, 3),
                (375, 4),
                (65_913, 6),
            ]
        );
    }

    #[test]
    fn runs_split_at_valleys_and_drop_lone_bins() {
        let mut peaks = Vec::new();
        let mut profile = Vec::new();
        let run = [
            (10, 10),
            (11, 100),
            (12, 10),
            (13, 2),
            (14, 20),
            (15, 60),
            (16, 20),
        ];
        centroids(&run, &mut profile, |weight, weighted| {
            peaks.push((weight, weighted / weight))
        });
        assert_eq!(peaks.len(), 2);
        assert_eq!(peaks[0], (122.0, (100.0 + 1100.0 + 120.0 + 26.0) / 122.0));
        assert_eq!(peaks[1].0, 100.0);
        // a shallow dip stays one peak, a lone bin is noise
        let mut weights = Vec::new();
        centroids(&[(1, 50), (2, 45), (3, 50)], &mut profile, |weight, _| {
            weights.push(weight)
        });
        centroids(&[(9, 50)], &mut profile, |weight, _| weights.push(weight));
        assert_eq!(weights, [145.0]);
    }

    #[test]
    fn truncated_and_unknown_streams_fail() {
        assert!(decode_bins(&[0x05, 0x7d, 0x01], 0, |_, _| {}).is_err());
        assert!(decode_bins(&[0x05], 0, |_, _| {}).is_err());
        assert!(decode_bins(&[0x7f, 0xff], 0, |_, _| {}).is_err());
    }

    #[test]
    fn calibration_ignores_index_fields() {
        let mut header = vec![0x0a, 0x12, 0x09];
        header.extend(0.0005_f64.to_le_bytes());
        header.push(0x11);
        header.extend((-13.0_f64).to_le_bytes());
        header.extend([0x12, 0x03, 0x08, 0x96, 0x01]);
        let calibration = parse_calibration(&header).unwrap();
        assert_eq!((calibration.a, calibration.t0), (0.0005, -13.0));
        assert!(parse_calibration(&header[..10]).is_none());
    }
}
