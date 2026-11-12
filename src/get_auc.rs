use crate::common::{Acq, DATA_DIR, Param, read_acq_time};
use crate::wiff::{Acquisition, Target, extract_xics, read_acquisition};
use std::error::Error;
use std::io::Write;
use std::path::Path;
const MISCDIR: &str = "misc";
// ms2 transitions match the experiment whose precursor is this close to q1
const PRECURSOR_TOL: f64 = 0.5;
const DEFAULT_HALF_WIDTH: f64 = 0.01;

pub fn calc_auc(param_t: &Param) -> Result<(), Box<dyn Error>> {
    std::fs::create_dir_all(MISCDIR)?;
    let t_list = read_assay(&param_t.t_list, param_t.mass_tolerance)?;
    let (_, acq_time) = read_acq_time()?;
    let dat_l: Vec<_> = acq_time
        .iter()
        .map(|acq| parse_auc(acq, &t_list).map_err(|error| format!("{}: {error}", acq.file)))
        .collect::<Result<_, _>>()?;
    write_long(&dat_l, &acq_time, &t_list)?;
    Ok(())
}

fn write_long(dat_l: &[Vec<Vec<RowDat>>], acq_time: &[Acq], t_list: &[QQ]) -> std::io::Result<()> {
    let mut wtr = csv::WriterBuilder::new().from_path("long.csv")?;
    wtr.write_record([
        "sample",
        "ID",
        "well",
        "window start",
        "window end",
        "RT at apex",
        "area",
        "height",
    ])?;
    for (Acq { file, windows }, dat_l_f) in acq_time.iter().zip(dat_l) {
        for (qq, dat_l_f_c) in t_list.iter().zip(dat_l_f) {
            for (dat, (label, beg, end)) in dat_l_f_c.iter().zip(windows) {
                wtr.write_record([
                    file,
                    &qq.name,
                    label,
                    &format!("{beg:.3}"),
                    &format!("{end:.3}"),
                    &format!("{:.3}", dat.rt),
                    &format!("{:.1}", dat.auc),
                    &format!("{:.1}", dat.height),
                ])?;
            }
        }
    }
    wtr.flush()
}

fn parse_auc(
    Acq { file, windows }: &Acq,
    t_list: &[QQ],
) -> Result<Vec<Vec<RowDat>>, Box<dyn Error>> {
    println!("{file:?}");
    let wiff = Path::new(DATA_DIR).join(file);
    let acquisition = read_acquisition(&wiff)?;
    let targets = t_list
        .iter()
        .map(|trans| target(trans, &acquisition))
        .collect::<Result<Vec<_>, _>>()?;
    let xics = extract_xics(&wiff, &acquisition, &targets)?;

    // build the whole plot first so a failure never leaves a partial file behind
    let mut bufp = Vec::new();
    bufp.write_all(&u16::try_from(t_list.len())?.to_le_bytes())?;
    let rows = t_list
        .iter()
        .zip(&xics)
        .map(|(trans, xic)| print_plot(&mut bufp, trans, xic, windows))
        .collect::<Result<_, _>>()?;
    std::fs::write(Path::new(MISCDIR).join(format!("plot_{file}.bin")), bufp)?;
    Ok(rows)
}

fn target(trans: &QQ, acquisition: &Acquisition) -> Result<Target, String> {
    let experiments = acquisition.experiments.iter().enumerate();
    let (experiment, mz) = match trans.q3 {
        None => experiments
            .clone()
            .find(|(_, experiment)| experiment.ms_level == 1)
            .map(|(index, _)| (index, trans.q1))
            .ok_or_else(|| {
                format!(
                    "{} needs a TOF MS experiment, none was acquired",
                    trans.name
                )
            })?,
        Some(q3) => experiments
            .filter(|(_, experiment)| experiment.ms_level == 2)
            .map(|(index, experiment)| (index, (experiment.precursor - trans.q1).abs()))
            .filter(|(_, distance)| *distance <= PRECURSOR_TOL)
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(index, _)| (index, q3))
            .ok_or_else(|| {
                format!(
                    "Transition {} has no MS/MS experiment with precursor {}",
                    trans.name, trans.q1
                )
            })?,
    };
    Ok(Target {
        experiment,
        mz,
        half_width: trans.half_width,
    })
}

/* the integration takes every scan segment the window overlaps, so a window
edge that falls between scans takes in the whole ramp on both sides instead of
whichever scan happens to be nearer. bounds saved from the visualizer sit on
scan times but are written to 3 decimals, SNAP keeps them on the same scan */
const SNAP: f32 = 0.001;

fn window_points(xic: &[(f32, f32)], beg: f32, end: f32) -> (usize, usize) {
    let first = xic
        .partition_point(|point| point.0 <= beg + SNAP)
        .saturating_sub(1);
    let last = xic
        .partition_point(|point| point.0 < end - SNAP)
        .clamp(first, xic.len() - 1);
    (first, last)
}

fn print_plot(
    bufp: &mut impl Write,
    trans: &QQ,
    rt_i_l: &[(f32, f32)],
    windows: &[(String, f32, f32)],
) -> Result<Vec<RowDat>, Box<dyn Error>> {
    if rt_i_l.is_empty() {
        return Err(format!("Transition {} has no scans", trans.name).into());
    }
    bufp.write_all(trans.name.as_bytes())?;
    bufp.write_all(b"\0")?;
    bufp.write_all(&u32::try_from(rt_i_l.len())?.to_le_bytes())?;
    for x in rt_i_l {
        bufp.write_all(&x.0.to_le_bytes())?;
        bufp.write_all(&x.1.to_le_bytes())?;
    }
    bufp.write_all(&u16::try_from(windows.len())?.to_le_bytes())?;
    windows
        .iter()
        .map(|(lab, beg, end)| {
            bufp.write_all(lab.as_bytes())?;
            bufp.write_all(b"\0")?;
            let (pos0, last) = window_points(rt_i_l, *beg, *end);
            let pos1 = last + 1;
            bufp.write_all(&u32::try_from(pos0 + 1)?.to_le_bytes())?;
            bufp.write_all(&u32::try_from(pos1)?.to_le_bytes())?;
            // minutes to seconds, halved for the trapezoids
            let auc = rt_i_l[pos0..pos1]
                .windows(2)
                .map(|x| (x[0].1 + x[1].1) * (x[1].0 - x[0].0))
                .sum::<f32>()
                * 30.;
            let (rt, height): (f32, f32) = rt_i_l[pos0..pos1]
                .iter()
                .max_by(|x, y| x.1.total_cmp(&y.1))
                .map_or((*beg, 0.), |x| *x);
            Ok(RowDat { auc, height, rt })
        })
        .collect()
}

struct RowDat {
    auc: f32,
    height: f32,
    rt: f32,
}

struct QQ {
    name: String,
    q1: f64,
    // none for TOF MS traces
    q3: Option<f64>,
    // ± Da around the target mass
    half_width: f64,
}

struct Columns {
    name: usize,
    q1: usize,
    q3: Option<usize>,
    width: Option<usize>,
}

/* columns are found by header so SCIEX style sheets ("Precursor (Q1) Mass (Da)",
"Fragment (Q3) Mass (Da)", "XIC Width (Da)") and plain name,q1,q3 lists both
load. without recognizable headers the first three columns are used */
fn columns(headers: &csv::StringRecord) -> Columns {
    let headers = headers.iter().map(str::to_lowercase).collect::<Vec<_>>();
    let find = |keys: &[&str]| {
        headers
            .iter()
            .position(|header| keys.iter().any(|key| header.contains(key)))
    };
    let width = find(&["xic", "width"]);
    let q3 = find(&["q3", "fragment", "product", "daughter"]);
    let q1 = find(&["q1", "precursor", "parent"]);
    let name = headers
        .iter()
        .position(|header| header == "name" || header == "id")
        .or_else(|| find(&["compound", "transition", "name"]));
    match q1 {
        Some(q1) => Columns {
            name: name.unwrap_or(0),
            q1,
            q3,
            width,
        },
        None => Columns {
            name: 0,
            q1: 1,
            q3: Some(2),
            width: None,
        },
    }
}

/* the xic is q ± half the transition's xic width (SCIEX sheets give the full
width), ± 0.01 Da without one. a mass_tolerance in param.txt replaces all of
them, the way RFkit uses one tolerance for every transition */
fn read_assay(assay_f: &Path, tolerance: Option<f64>) -> Result<Vec<QQ>, Box<dyn Error>> {
    let mut rdr = csv::ReaderBuilder::new()
        .comment(Some(b'#'))
        .has_headers(true)
        .flexible(true)
        .trim(csv::Trim::All)
        .from_path(assay_f)?;
    let columns = columns(rdr.headers()?);
    let mut t_list = Vec::new();
    for record in rdr.records() {
        let record = record?;
        let line = record.position().map_or(0, |position| position.line());
        if record.iter().all(str::is_empty) {
            continue;
        }
        let field =
            |column: Option<usize>| column.and_then(|column| record.get(column)).unwrap_or("");
        let mass = |column: Option<usize>, label: &str| -> Result<Option<f64>, String> {
            let text = field(column);
            if text.is_empty() {
                return Ok(None);
            }
            let value: f64 = text
                .parse()
                .map_err(|err| format!("Invalid {label} in transition line {line}: {err}"))?;
            if !value.is_finite() || value < 0.0 {
                return Err(format!("Invalid {label} in transition line {line}"));
            }
            Ok(Some(value))
        };
        let name = field(Some(columns.name));
        let q1 = mass(Some(columns.q1), "Q1")?
            .filter(|q1| *q1 > 0.0)
            .ok_or_else(|| format!("Transition line {line} requires a Q1 mass"))?;
        if name.is_empty() {
            return Err(format!("Transition line {line} requires a name").into());
        }
        // a blank or zero q3 traces the precursor in TOF MS
        let q3 = mass(columns.q3, "Q3")?.filter(|q3| *q3 > 0.0);
        let half_width = match mass(columns.width, "XIC width")? {
            Some(width) if width > 0.0 => width / 2.0,
            Some(_) => {
                return Err(format!("XIC width must be positive in transition line {line}").into());
            }
            None => DEFAULT_HALF_WIDTH,
        };
        t_list.push(QQ {
            name: name.to_string(),
            q1,
            q3,
            half_width: tolerance.unwrap_or(half_width),
        });
    }
    if t_list.is_empty() {
        return Err("Transition list contains no transitions".into());
    }
    Ok(t_list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sciex_headers_map_by_name() {
        let headers = csv::StringRecord::from(vec![
            "No.",
            "MS_Lvl",
            "Group",
            "Name",
            "Adduct/Charge",
            "Precursor (Q1) Mass (Da)",
            "Fragment (Q3) Mass (Da)",
            "XIC Width (Da)",
        ]);
        let columns = columns(&headers);
        assert_eq!(
            (columns.name, columns.q1, columns.q3, columns.width),
            (3, 5, Some(6), Some(7))
        );
    }

    #[test]
    fn unknown_headers_fall_back_to_position() {
        let columns = columns(&csv::StringRecord::from(vec!["a", "b", "c"]));
        assert_eq!((columns.name, columns.q1, columns.q3), (0, 1, Some(2)));
    }

    #[test]
    fn windows_take_in_every_overlapped_scan() {
        let xic = [(0.0, 0.0), (0.1, 10.0), (0.2, 20.0), (0.3, 0.0)];
        let mut buf = Vec::new();
        let trans = QQ {
            name: "x".into(),
            q1: 1.0,
            q3: None,
            half_width: 0.01,
        };
        let windows = [("w".into(), 0.09, 0.21), ("saved".into(), 0.1004, 0.1996)];
        let rows = print_plot(&mut buf, &trans, &xic, &windows).unwrap();
        // both ramps count, trapezoids in intensity seconds
        assert!((rows[0].auc - 60.0 * 0.1 * 60.0 / 2.0).abs() < 1e-3);
        assert_eq!((rows[0].rt, rows[0].height), (0.2, 20.0));
        // bounds rounded off scan times stay on those scans
        assert!((rows[1].auc - 30.0 * 0.1 * 60.0 / 2.0).abs() < 1e-3);
        let tail = &buf[buf.len() - 8..];
        assert_eq!(tail, [2, 0, 0, 0, 3, 0, 0, 0]);
        assert_eq!(window_points(&xic, 0.5, 0.6), (3, 3));
        assert_eq!(window_points(&xic, -1.0, -0.5), (0, 0));
    }
}
