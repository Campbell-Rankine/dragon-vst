//! Headless CLAP smoke test: load a `.clap` bundle, instantiate every plugin in
//! it, run audio through it, check the output level, and save its state.
//! Exits non-zero on any failure.
//!
//! Effects (plugins with an audio input) get a stereo sine and are checked for
//! the level change relative to it. Instruments (no audio input) get a note
//! instead: by default A4 (MIDI 69) at full velocity, released halfway through.
//! They're checked for sound while the note is held (peak in dBFS within
//! `--min-db` / `--max-db`) and for silence at the end (no stuck notes).
//!
//! ```text
//! clap-smoke <Plugin.clap> [options]
//!
//!   --set "<param name>=<text>"   set a parameter before processing (repeatable);
//!                                 the text is parsed by the plugin, e.g. "12",
//!                                 "Low Cut", "On" / "Off", "48 dB/oct"
//!
//! Effects:
//!   --tone <Hz>                   test tone frequency (default 1000)
//!   --expect-db <dB>              expected output level change, both channels (default 0)
//!   --expect-left-db <dB>         override for the left channel
//!   --expect-right-db <dB>        override for the right channel
//!   --tol <dB>                    allowed error (default 0.05)
//!
//! Instruments (also usable on effects that accept notes):
//!   --note <key>                  MIDI key to play (default 69 for instruments)
//!   --velocity <0..1>             note-on velocity (default 1)
//!   --hold                        never send note-off (skips the silence check)
//!   --min-db <dBFS>               minimum held output peak (default -60 for instruments)
//!   --max-db <dBFS>               maximum held output peak (default 0 for instruments)
//! ```
//!
//! Examples:
//! ```text
//! cargo run -p clap-smoke -- "target/bundled/Dragon 24EQ.clap"
//! cargo run -p clap-smoke -- "target/bundled/Dragon 24EQ.clap" --set "Band 2 Gain=12" --expect-db 12
//! cargo run -p clap-smoke -- "target/bundled/Dragon Waave.clap" --note 48 --min-db -20
//! ```

use clap_sys::audio_buffer::clap_audio_buffer;
use clap_sys::entry::clap_plugin_entry;
use clap_sys::events::{
    clap_event_header, clap_event_note, clap_event_param_value, clap_input_events, clap_output_events,
    CLAP_CORE_EVENT_SPACE_ID, CLAP_EVENT_NOTE_OFF, CLAP_EVENT_NOTE_ON, CLAP_EVENT_PARAM_VALUE,
};
use clap_sys::ext::audio_ports::{clap_plugin_audio_ports, CLAP_EXT_AUDIO_PORTS};
use clap_sys::ext::note_ports::{clap_plugin_note_ports, CLAP_EXT_NOTE_PORTS};
use clap_sys::ext::params::{clap_param_info, clap_plugin_params, CLAP_EXT_PARAMS};
use clap_sys::ext::state::{clap_plugin_state, CLAP_EXT_STATE};
use clap_sys::factory::plugin_factory::{clap_plugin_factory, CLAP_PLUGIN_FACTORY_ID};
use clap_sys::host::clap_host;
use clap_sys::plugin::clap_plugin;
use clap_sys::process::{clap_process, CLAP_PROCESS_ERROR};
use clap_sys::stream::clap_ostream;
use clap_sys::version::CLAP_VERSION;
use std::ffi::{c_char, c_void, CStr, CString};
use std::path::PathBuf;
use std::process::ExitCode;

const SAMPLE_RATE: f64 = 48_000.0;
const BLOCK: usize = 512;
const BLOCKS: usize = 100;
/// Blocks skipped before measuring, so parameter smoothing has settled.
const SETTLE_BLOCKS: usize = 20;
const INPUT_AMPLITUDE: f32 = 0.5;
/// Block the note-off is sent in (about 0.53 s in).
const NOTE_OFF_BLOCK: usize = 50;
/// The last this-many blocks must be silent after a note-off (about 0.1 s).
const TAIL_BLOCKS: usize = 10;
/// Peak level (dBFS) that counts as silence.
const SILENCE_DB: f64 = -80.0;
const DEFAULT_NOTE: u8 = 69;
const INSTRUMENT_MIN_DB: f64 = -60.0;
const INSTRUMENT_MAX_DB: f64 = 0.0;

macro_rules! check {
    ($cond:expr, $($msg:tt)*) => {
        if !$cond {
            return Err(format!($($msg)*));
        }
    };
}

struct Options {
    bundle: PathBuf,
    sets: Vec<(String, String)>,
    tone: f32,
    /// `None` unless given; instruments reject them.
    expect_left_db: Option<f64>,
    expect_right_db: Option<f64>,
    tol: f64,
    note: Option<u8>,
    velocity: f64,
    hold: bool,
    min_db: Option<f64>,
    max_db: Option<f64>,
}

fn parse_args() -> Result<Options, String> {
    let mut args = std::env::args().skip(1);
    let bundle = PathBuf::from(args.next().ok_or("missing <Plugin.clap>")?);
    let mut opts = Options {
        bundle,
        sets: Vec::new(),
        tone: 1000.0,
        expect_left_db: None,
        expect_right_db: None,
        tol: 0.05,
        note: None,
        velocity: 1.0,
        hold: false,
        min_db: None,
        max_db: None,
    };
    let (mut left, mut right, mut both) = (None, None, None);
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or(format!("{flag} needs a value"));
        let num = |s: String| s.parse::<f64>().map_err(|e| format!("bad number {s:?}: {e}"));
        match flag.as_str() {
            "--set" => {
                let v = value()?;
                let (name, text) = v.split_once('=').ok_or(format!("--set expects NAME=TEXT, got {v:?}"))?;
                opts.sets.push((name.to_string(), text.to_string()));
            }
            "--tone" => opts.tone = num(value()?)? as f32,
            "--expect-db" => both = Some(num(value()?)?),
            "--expect-left-db" => left = Some(num(value()?)?),
            "--expect-right-db" => right = Some(num(value()?)?),
            "--tol" => opts.tol = num(value()?)?,
            "--note" => {
                let v = value()?;
                let key = v.parse::<u8>().map_err(|e| format!("bad note {v:?}: {e}"))?;
                check!(key <= 127, "--note must be 0..=127, got {key}");
                opts.note = Some(key);
            }
            "--velocity" => {
                opts.velocity = num(value()?)?;
                check!((0.0..=1.0).contains(&opts.velocity), "--velocity must be 0..=1");
            }
            "--hold" => opts.hold = true,
            "--min-db" => opts.min_db = Some(num(value()?)?),
            "--max-db" => opts.max_db = Some(num(value()?)?),
            other => return Err(format!("unknown option {other}")),
        }
    }
    opts.expect_left_db = left.or(both);
    opts.expect_right_db = right.or(both);
    Ok(opts)
}

unsafe extern "C" fn host_get_extension(_: *const clap_host, _: *const c_char) -> *const c_void {
    std::ptr::null()
}
unsafe extern "C" fn host_noop(_: *const clap_host) {}

/// An input event of any type we send.
enum Event {
    Param(clap_event_param_value),
    Note(clap_event_note),
}

impl Event {
    fn header(&self) -> &clap_event_header {
        match self {
            Event::Param(e) => &e.header,
            Event::Note(e) => &e.header,
        }
    }
}

/// Input event list backed by a `Vec<Event>` in `ctx`.
unsafe extern "C" fn in_events_size(list: *const clap_input_events) -> u32 {
    let events = &*((*list).ctx as *const Vec<Event>);
    events.len() as u32
}
unsafe extern "C" fn in_events_get(list: *const clap_input_events, index: u32) -> *const clap_event_header {
    let events = &*((*list).ctx as *const Vec<Event>);
    events[index as usize].header()
}
unsafe extern "C" fn out_events_push(_: *const clap_output_events, _: *const clap_event_header) -> bool {
    true
}

unsafe extern "C" fn ostream_write(stream: *const clap_ostream, buf: *const c_void, size: u64) -> i64 {
    let out = &mut *((*stream).ctx as *mut Vec<u8>);
    out.extend_from_slice(std::slice::from_raw_parts(buf as *const u8, size as usize));
    size as i64
}

fn main() -> ExitCode {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!(
                "error: {e}\nusage: clap-smoke <Plugin.clap> [--set NAME=TEXT]... [--tone HZ] [--expect-db DB] \
                 [--expect-left-db DB] [--expect-right-db DB] [--tol DB] [--note KEY] [--velocity V] [--hold] \
                 [--min-db DBFS] [--max-db DBFS]"
            );
            return ExitCode::from(2);
        }
    };
    match unsafe { run(&opts) } {
        Ok(()) => {
            println!("PASS");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("FAIL: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Resolve `--set` names to param-value events via the params extension.
unsafe fn param_events(plugin: *const clap_plugin, sets: &[(String, String)]) -> Result<Vec<Event>, String> {
    if sets.is_empty() {
        return Ok(Vec::new());
    }
    let p = &*plugin;
    let ext = (p.get_extension.unwrap())(plugin, CLAP_EXT_PARAMS.as_ptr()) as *const clap_plugin_params;
    check!(!ext.is_null(), "plugin has no params extension");
    let ext = &*ext;
    let count = (ext.count.unwrap())(plugin);

    let mut events = Vec::new();
    for (name, text) in sets {
        let mut found = None;
        for i in 0..count {
            let mut info: clap_param_info = std::mem::zeroed();
            if (ext.get_info.unwrap())(plugin, i, &mut info) && CStr::from_ptr(info.name.as_ptr()).to_str() == Ok(name) {
                found = Some(info.id);
                break;
            }
        }
        let id = found.ok_or(format!("no parameter named {name:?}"))?;
        let text_c = CString::new(text.as_str()).unwrap();
        let mut value = 0.0;
        check!(
            (ext.text_to_value.unwrap())(plugin, id, text_c.as_ptr(), &mut value),
            "plugin could not parse {text:?} for {name:?}"
        );
        println!("  set {name} = {text} (value {value})");
        events.push(Event::Param(clap_event_param_value {
            header: clap_event_header {
                size: std::mem::size_of::<clap_event_param_value>() as u32,
                time: 0,
                space_id: CLAP_CORE_EVENT_SPACE_ID,
                type_: CLAP_EVENT_PARAM_VALUE,
                flags: 0,
            },
            param_id: id,
            cookie: std::ptr::null_mut(),
            note_id: -1,
            port_index: -1,
            channel: -1,
            key: -1,
            value,
        }));
    }
    Ok(events)
}

fn note_event(type_: u16, key: u8, velocity: f64) -> Event {
    Event::Note(clap_event_note {
        header: clap_event_header {
            size: std::mem::size_of::<clap_event_note>() as u32,
            time: 0,
            space_id: CLAP_CORE_EVENT_SPACE_ID,
            type_,
            flags: 0,
        },
        note_id: -1,
        port_index: 0,
        channel: 0,
        key: key as i16,
        velocity,
    })
}

/// Number of input ports (audio or note) a plugin reports for an extension
/// whose vtable starts with `count(plugin, is_input)`. Missing extension = 0.
unsafe fn input_port_count(plugin: *const clap_plugin, ext_id: &CStr, audio: bool) -> u32 {
    let p = &*plugin;
    let ext = (p.get_extension.unwrap())(plugin, ext_id.as_ptr());
    if ext.is_null() {
        return 0;
    }
    if audio {
        ((*(ext as *const clap_plugin_audio_ports)).count.unwrap())(plugin, true)
    } else {
        ((*(ext as *const clap_plugin_note_ports)).count.unwrap())(plugin, true)
    }
}

fn to_dbfs(peak: f32) -> f64 {
    20.0 * (peak as f64).log10()
}

unsafe fn run(opts: &Options) -> Result<(), String> {
    let bundle = &opts.bundle;
    // On macOS a .clap is a bundle; the binary lives in Contents/MacOS/<name>.
    let binary = if bundle.is_dir() {
        let stem = bundle.file_stem().ok_or("bad bundle path")?;
        bundle.join("Contents/MacOS").join(stem)
    } else {
        bundle.clone()
    };
    println!("loading {}", binary.display());
    let lib = libloading::Library::new(&binary).map_err(|e| format!("dlopen: {e}"))?;
    let entry: libloading::Symbol<*const clap_plugin_entry> =
        lib.get(b"clap_entry\0").map_err(|e| format!("clap_entry symbol: {e}"))?;
    let entry = &**entry;

    let path_c = CString::new(bundle.to_string_lossy().as_bytes()).unwrap();
    check!((entry.init.unwrap())(path_c.as_ptr()), "entry.init returned false");

    let factory = (entry.get_factory.unwrap())(CLAP_PLUGIN_FACTORY_ID.as_ptr()) as *const clap_plugin_factory;
    check!(!factory.is_null(), "no plugin factory");
    let count = ((*factory).get_plugin_count.unwrap())(factory);
    check!(count > 0, "factory reports 0 plugins");

    let host = clap_host {
        clap_version: CLAP_VERSION,
        host_data: std::ptr::null_mut(),
        name: c"clap-smoke".as_ptr(),
        vendor: c"dragon-vst".as_ptr(),
        url: c"".as_ptr(),
        version: c"0.1.0".as_ptr(),
        get_extension: Some(host_get_extension),
        request_restart: Some(host_noop),
        request_process: Some(host_noop),
        request_callback: Some(host_noop),
    };

    for i in 0..count {
        let desc = &*((*factory).get_plugin_descriptor.unwrap())(factory, i);
        let id = CStr::from_ptr(desc.id);
        println!("plugin {i}: {} ({})", CStr::from_ptr(desc.name).to_string_lossy(), id.to_string_lossy());

        let plugin = ((*factory).create_plugin.unwrap())(factory, &host, id.as_ptr());
        check!(!plugin.is_null(), "create_plugin returned null");
        let p = &*plugin;
        check!((p.init.unwrap())(plugin), "plugin.init returned false");
        println!("  init ok");

        // Effect or instrument?
        let has_audio_input = input_port_count(plugin, CLAP_EXT_AUDIO_PORTS, true) > 0;
        let note = opts.note.or((!has_audio_input).then_some(DEFAULT_NOTE));
        if !has_audio_input {
            check!(
                opts.expect_left_db.is_none() && opts.expect_right_db.is_none(),
                "--expect-*-db measures gain relative to the input; this plugin has no audio input, \
                 use --min-db / --max-db"
            );
        }
        if note.is_some() {
            check!(
                input_port_count(plugin, CLAP_EXT_NOTE_PORTS, false) > 0,
                "a note is to be played but the plugin has no note input port"
            );
        }
        let (min_db, max_db) = if has_audio_input {
            (opts.min_db, opts.max_db)
        } else {
            (Some(opts.min_db.unwrap_or(INSTRUMENT_MIN_DB)), Some(opts.max_db.unwrap_or(INSTRUMENT_MAX_DB)))
        };
        let note_off_block = note.filter(|_| !opts.hold).map(|_| NOTE_OFF_BLOCK);

        // Events per block: params (and note-on) first, then the note-off.
        let mut schedule: Vec<Vec<Event>> = (0..BLOCKS).map(|_| Vec::new()).collect();
        schedule[0] = param_events(plugin, &opts.sets)?;
        if let Some(key) = note {
            schedule[0].push(note_event(CLAP_EVENT_NOTE_ON, key, opts.velocity));
            if let Some(b) = note_off_block {
                schedule[b].push(note_event(CLAP_EVENT_NOTE_OFF, key, 0.0));
            }
        }

        check!(
            (p.activate.unwrap())(plugin, SAMPLE_RATE, 1, BLOCK as u32),
            "activate(48k, 512) returned false"
        );
        println!("  activate ok");

        // Processing happens on its own thread, like a real host's audio thread.
        let plugin_addr = plugin as usize;
        let tone = has_audio_input.then_some(opts.tone);
        let m = std::thread::spawn(move || {
            process_blocks(plugin_addr as *const clap_plugin, schedule, tone, note_off_block)
        })
        .join()
        .map_err(|_| "audio thread panicked".to_string())??;

        let (left_dbfs, right_dbfs) = (to_dbfs(m.peak_l), to_dbfs(m.peak_r));
        let what = match (tone, note) {
            (Some(hz), Some(key)) => format!("a {hz} Hz tone + note {key}"),
            (Some(hz), None) => format!("a {hz} Hz tone"),
            (None, Some(key)) => format!("note {key}"),
            (None, None) => "silence".to_string(),
        };

        if has_audio_input {
            let in_db = to_dbfs(INPUT_AMPLITUDE);
            let (left_db, right_db) = (left_dbfs - in_db, right_dbfs - in_db);
            let (exp_l, exp_r) = (opts.expect_left_db.unwrap_or(0.0), opts.expect_right_db.unwrap_or(0.0));
            println!(
                "  processed {BLOCKS} blocks of {what}: left {left_db:+.3} dB, right {right_db:+.3} dB \
                 (expected {exp_l:+.3} / {exp_r:+.3} ± {})",
                opts.tol
            );
            check!(
                (left_db - exp_l).abs() <= opts.tol && (right_db - exp_r).abs() <= opts.tol,
                "output level outside tolerance"
            );
        } else {
            println!("  processed {BLOCKS} blocks of {what}: peak left {left_dbfs:+.2} dBFS, right {right_dbfs:+.2} dBFS");
        }

        if min_db.is_some() || max_db.is_some() {
            let lo = min_db.unwrap_or(f64::NEG_INFINITY);
            let hi = max_db.unwrap_or(f64::INFINITY);
            println!("  held peak range check: {lo:+.2} .. {hi:+.2} dBFS");
            for (side, db) in [("left", left_dbfs), ("right", right_dbfs)] {
                check!(db >= lo, "{side} peak {db:+.2} dBFS is below {lo:+.2} dBFS (too quiet / silent)");
                check!(db <= hi, "{side} peak {db:+.2} dBFS is above {hi:+.2} dBFS");
            }
        }

        if note_off_block.is_some() {
            let tail_db = to_dbfs(m.tail_peak);
            println!("  after note-off: last {TAIL_BLOCKS} blocks peak {tail_db:+.2} dBFS (must be < {SILENCE_DB} dBFS)");
            check!(tail_db < SILENCE_DB, "output did not decay to silence after note-off (stuck note?)");
        }

        (p.deactivate.unwrap())(plugin);

        let state_ext = (p.get_extension.unwrap())(plugin, CLAP_EXT_STATE.as_ptr()) as *const clap_plugin_state;
        check!(!state_ext.is_null(), "plugin has no state extension");
        let mut bytes: Vec<u8> = Vec::new();
        let ostream = clap_ostream { ctx: &mut bytes as *mut _ as *mut c_void, write: Some(ostream_write) };
        check!(((*state_ext).save.unwrap())(plugin, &ostream), "state save returned false");
        println!("  state save ok ({} bytes)", bytes.len());

        (p.destroy.unwrap())(plugin);
        println!("  destroy ok");
    }

    (entry.deinit.unwrap())();
    Ok(())
}

/// Output peaks from one run.
struct Measurement {
    /// Settled peak per channel: from `SETTLE_BLOCKS` up to the note-off (or
    /// the end if there is none).
    peak_l: f32,
    peak_r: f32,
    /// Peak over both channels in the last `TAIL_BLOCKS` blocks.
    tail_peak: f32,
}

/// Run `BLOCKS` blocks through the plugin, sending `schedule[block]` with
/// each block. With `tone`, the input is a stereo sine (same on both
/// channels); without, the plugin gets no audio input.
unsafe fn process_blocks(
    plugin: *const clap_plugin,
    schedule: Vec<Vec<Event>>,
    tone: Option<f32>,
    note_off_block: Option<usize>,
) -> Result<Measurement, String> {
    let p = &*plugin;
    check!((p.start_processing.unwrap())(plugin), "start_processing returned false");

    let mut in_l = vec![0f32; BLOCK];
    let mut in_r = vec![0f32; BLOCK];
    let mut out_l = vec![0f32; BLOCK];
    let mut out_r = vec![0f32; BLOCK];
    let event_lists: Vec<clap_input_events> = schedule
        .iter()
        .map(|events| clap_input_events {
            ctx: events as *const _ as *mut c_void,
            size: Some(in_events_size),
            get: Some(in_events_get),
        })
        .collect();
    let out_events = clap_output_events { ctx: std::ptr::null_mut(), try_push: Some(out_events_push) };

    let measure_end = note_off_block.unwrap_or(BLOCKS);
    let mut m = Measurement { peak_l: 0.0, peak_r: 0.0, tail_peak: 0.0 };
    let mut phase = 0f64;
    let step = 2.0 * std::f64::consts::PI * tone.unwrap_or(0.0) as f64 / SAMPLE_RATE;
    for (block, in_events) in event_lists.iter().enumerate() {
        if tone.is_some() {
            for i in 0..BLOCK {
                let s = INPUT_AMPLITUDE * phase.sin() as f32;
                in_l[i] = s;
                in_r[i] = s;
                phase += step;
            }
        }
        let mut in_ptrs = [in_l.as_mut_ptr(), in_r.as_mut_ptr()];
        let mut out_ptrs = [out_l.as_mut_ptr(), out_r.as_mut_ptr()];
        let inputs = clap_audio_buffer {
            data32: in_ptrs.as_mut_ptr(),
            data64: std::ptr::null_mut(),
            channel_count: 2,
            latency: 0,
            constant_mask: 0,
        };
        let mut outputs = clap_audio_buffer {
            data32: out_ptrs.as_mut_ptr(),
            data64: std::ptr::null_mut(),
            channel_count: 2,
            latency: 0,
            constant_mask: 0,
        };
        let process = clap_process {
            steady_time: (block * BLOCK) as i64,
            frames_count: BLOCK as u32,
            transport: std::ptr::null(),
            audio_inputs: if tone.is_some() { &inputs } else { std::ptr::null() },
            audio_outputs: &mut outputs,
            audio_inputs_count: tone.is_some() as u32,
            audio_outputs_count: 1,
            in_events,
            out_events: &out_events,
        };
        let status = (p.process.unwrap())(plugin, &process);
        check!(status != CLAP_PROCESS_ERROR, "process returned CLAP_PROCESS_ERROR on block {block}");
        for (&y, out) in out_l.iter().zip(&out_r) {
            check!(y.is_finite() && out.is_finite(), "non-finite output on block {block}");
        }
        if (SETTLE_BLOCKS..measure_end).contains(&block) {
            m.peak_l = out_l.iter().fold(m.peak_l, |acc, s| acc.max(s.abs()));
            m.peak_r = out_r.iter().fold(m.peak_r, |acc, s| acc.max(s.abs()));
        }
        if block >= BLOCKS - TAIL_BLOCKS {
            m.tail_peak = out_l.iter().chain(&out_r).fold(m.tail_peak, |acc, s| acc.max(s.abs()));
        }
    }

    (p.stop_processing.unwrap())(plugin);
    Ok(m)
}
