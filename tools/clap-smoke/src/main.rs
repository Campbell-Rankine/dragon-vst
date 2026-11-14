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
//!   --chord <n>                   play n notes (the key, then every 2 semitones up; default 1)
//!   --seconds <s>                 run length (default about 1.07 s; never shorter)
//!   --note-at <samples>           start the note(s) this late (default 0). Levels are then
//!                                 measured from the note-on, so the attack counts; e.g.
//!                                 10240 lets --set values finish smoothing first
//!   --note-every <samples>        repeat the note-on(s) this often for the whole run (e.g.
//!                                 a kick pattern for a MIDI-triggered effect; use --hold)
//!
//! Other:
//!   --list-params                 print every parameter's display name and exit
//!
//! Tempo-synced plugins:
//!   --tempo <bpm>                 send a playing transport from beat 0 (default: none,
//!                                 as from a host that reports no transport)
//!   --trough-max-db <dB>          the quietest 10 ms window (after settling; relative to the
//!                                 input for effects, dBFS for instruments) must be at or below this
//!   --expect-trough-db <dB>       ...or within --tol of this
//!
//! Sidechain plugins:
//!   --sidechain-kicks <bpm>       feed every sidechain (aux) input synthetic kicks on each
//!                                 beat (a 60 Hz sine decaying over 80 ms, peak about -2 dBFS)
//!                                 instead of silence
//!
//! Load (any plugin):
//!   --block <samples>             host buffer size (default 512); the run length stays the same
//!   --automate "<name>=<a>..<b>"  sweep a parameter between two values (texts parsed by the
//!                                 plugin) as a triangle, one event every --automate-every
//!                                 samples (repeatable)
//!   --automate-every <samples>    spacing of automation events (default 16)
//!   --max-load <percent>          fail if the 99th-percentile process() time exceeds this
//!                                 share of the buffer's real-time budget
//! ```
//!
//! Examples:
//! ```text
//! cargo run -p clap-smoke -- "target/bundled/Dragon 24EQ.clap"
//! cargo run -p clap-smoke -- "target/bundled/Dragon 24EQ.clap" --set "Band 2 Gain=12" --expect-db 12
//! cargo run -p clap-smoke -- "target/bundled/Dragon Waave.clap" --note 48 --min-db -20
//! cargo run -p clap-smoke --release -- "target/bundled/Dragon Waave.clap" --chord 16 --max-db 12 \
//!     --block 128 --automate "Filter Cutoff=200..5000" --max-load 30
//! ```

use clap_sys::audio_buffer::clap_audio_buffer;
use clap_sys::entry::clap_plugin_entry;
use clap_sys::events::{
    clap_event_header, clap_event_note, clap_event_param_value, clap_event_transport, clap_input_events,
    clap_output_events, CLAP_CORE_EVENT_SPACE_ID, CLAP_EVENT_NOTE_OFF, CLAP_EVENT_NOTE_ON, CLAP_EVENT_PARAM_VALUE,
    CLAP_EVENT_TRANSPORT, CLAP_TRANSPORT_HAS_BEATS_TIMELINE, CLAP_TRANSPORT_HAS_TEMPO, CLAP_TRANSPORT_IS_PLAYING,
};
use clap_sys::fixedpoint::CLAP_BEATTIME_FACTOR;
use clap_sys::ext::audio_ports::{clap_audio_port_info, clap_plugin_audio_ports, CLAP_EXT_AUDIO_PORTS};
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
use std::time::Instant;

const SAMPLE_RATE: f64 = 48_000.0;
const DEFAULT_BLOCK: usize = 512;
/// Length of a run (about 1.07 s), whatever the block size.
const RUN_SAMPLES: usize = 100 * 512;
/// Samples skipped before measuring levels, so parameter smoothing has settled.
const SETTLE_SAMPLES: usize = 20 * 512;
const INPUT_AMPLITUDE: f32 = 0.5;
/// When the note-off is sent (about 0.53 s in).
const NOTE_OFF_SAMPLES: usize = 50 * 512;
/// The last this-many samples must be silent after a note-off (about 0.1 s).
const TAIL_SAMPLES: usize = 10 * 512;
/// One up-and-down automation sweep (0.5 s), like a knob being wiggled.
const SWEEP_SAMPLES: usize = 24_000;
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
    chord: usize,
    block: usize,
    /// (name, from text, to text)
    automate: Vec<(String, String, String)>,
    automate_every: usize,
    max_load: Option<f64>,
    list_params: bool,
    note_at: usize,
    note_every: Option<usize>,
    run_samples: usize,
    tempo: Option<f64>,
    sidechain_kicks: Option<f64>,
    trough_max_db: Option<f64>,
    expect_trough_db: Option<f64>,
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
        chord: 1,
        block: DEFAULT_BLOCK,
        automate: Vec::new(),
        automate_every: 16,
        max_load: None,
        list_params: false,
        note_at: 0,
        note_every: None,
        run_samples: RUN_SAMPLES,
        tempo: None,
        sidechain_kicks: None,
        trough_max_db: None,
        expect_trough_db: None,
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
            "--chord" => {
                opts.chord = num(value()?)? as usize;
                check!((1..=64).contains(&opts.chord), "--chord must be 1..=64");
            }
            "--block" => {
                opts.block = num(value()?)? as usize;
                check!((1..=4096).contains(&opts.block), "--block must be 1..=4096");
            }
            "--automate" => {
                let v = value()?;
                let parsed = v
                    .split_once('=')
                    .and_then(|(name, range)| range.split_once("..").map(|(a, b)| (name, a, b)));
                let (name, a, b) = parsed.ok_or(format!("--automate expects NAME=FROM..TO, got {v:?}"))?;
                opts.automate.push((name.to_string(), a.to_string(), b.to_string()));
            }
            "--automate-every" => {
                opts.automate_every = num(value()?)? as usize;
                check!(opts.automate_every >= 1, "--automate-every must be at least 1");
            }
            "--max-load" => opts.max_load = Some(num(value()?)?),
            "--list-params" => opts.list_params = true,
            "--tempo" => {
                let bpm = num(value()?)?;
                check!(bpm > 0.0 && bpm <= 999.0, "--tempo must be in (0, 999]");
                opts.tempo = Some(bpm);
            }
            "--sidechain-kicks" => {
                let bpm = num(value()?)?;
                check!(bpm > 0.0 && bpm <= 999.0, "--sidechain-kicks must be in (0, 999]");
                opts.sidechain_kicks = Some(bpm);
            }
            "--trough-max-db" => opts.trough_max_db = Some(num(value()?)?),
            "--expect-trough-db" => opts.expect_trough_db = Some(num(value()?)?),
            "--seconds" => {
                let secs = num(value()?)?;
                check!(secs > 0.0 && secs <= 60.0, "--seconds must be in (0, 60]");
                opts.run_samples = ((secs * SAMPLE_RATE) as usize).max(RUN_SAMPLES);
            }
            "--note-every" => {
                let every = num(value()?)? as usize;
                check!(every >= 1, "--note-every must be at least 1 sample");
                opts.note_every = Some(every);
            }
            "--note-at" => {
                opts.note_at = num(value()?)? as usize;
                check!(opts.note_at < NOTE_OFF_SAMPLES, "--note-at must be before the note-off at {NOTE_OFF_SAMPLES}");
            }
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
                 [--min-db DBFS] [--max-db DBFS] [--chord N] [--block SAMPLES] [--automate NAME=FROM..TO]... \
                 [--automate-every SAMPLES] [--max-load PERCENT] [--list-params] [--note-at SAMPLES] [--note-every SAMPLES] [--seconds S] [--tempo BPM] [--sidechain-kicks BPM] \
                 [--trough-max-db DB] [--expect-trough-db DB]"
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

/// Look up a parameter by display name and parse `text` to its value.
unsafe fn resolve_param(plugin: *const clap_plugin, name: &str, text: &str) -> Result<(u32, f64), String> {
    let p = &*plugin;
    let ext = (p.get_extension.unwrap())(plugin, CLAP_EXT_PARAMS.as_ptr()) as *const clap_plugin_params;
    check!(!ext.is_null(), "plugin has no params extension");
    let ext = &*ext;
    let count = (ext.count.unwrap())(plugin);
    let mut found = None;
    for i in 0..count {
        let mut info: clap_param_info = std::mem::zeroed();
        if (ext.get_info.unwrap())(plugin, i, &mut info) && CStr::from_ptr(info.name.as_ptr()).to_str() == Ok(name) {
            found = Some(info.id);
            break;
        }
    }
    let id = found.ok_or(format!("no parameter named {name:?}"))?;
    let text_c = CString::new(text).unwrap();
    let mut value = 0.0;
    check!(
        (ext.text_to_value.unwrap())(plugin, id, text_c.as_ptr(), &mut value),
        "plugin could not parse {text:?} for {name:?}"
    );
    Ok((id, value))
}

/// Resolve `--set` names to param-value events at time 0.
unsafe fn param_events(plugin: *const clap_plugin, sets: &[(String, String)]) -> Result<Vec<Event>, String> {
    let mut events = Vec::new();
    for (name, text) in sets {
        let (id, value) = resolve_param(plugin, name, text)?;
        println!("  set {name} = {text} (value {value})");
        events.push(param_event(id, value, 0));
    }
    Ok(events)
}

fn param_event(param_id: u32, value: f64, time: u32) -> Event {
    Event::Param(clap_event_param_value {
        header: clap_event_header {
            size: std::mem::size_of::<clap_event_param_value>() as u32,
            time,
            space_id: CLAP_CORE_EVENT_SPACE_ID,
            type_: CLAP_EVENT_PARAM_VALUE,
            flags: 0,
        },
        param_id,
        cookie: std::ptr::null_mut(),
        note_id: -1,
        port_index: -1,
        channel: -1,
        key: -1,
        value,
    })
}

fn note_event(type_: u16, key: u8, velocity: f64, time: u32) -> Event {
    Event::Note(clap_event_note {
        header: clap_event_header {
            size: std::mem::size_of::<clap_event_note>() as u32,
            time,
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

/// Channel count of each audio input port, main first. Hosts must pass a
/// buffer for every declared port (nih-plug reads them unchecked), so
/// sidechain ports get silence unless a test feeds them.
unsafe fn input_port_channels(plugin: *const clap_plugin) -> Vec<u32> {
    let p = &*plugin;
    let ext = (p.get_extension.unwrap())(plugin, CLAP_EXT_AUDIO_PORTS.as_ptr()) as *const clap_plugin_audio_ports;
    if ext.is_null() {
        return Vec::new();
    }
    let ext = &*ext;
    (0..(ext.count.unwrap())(plugin, true))
        .map(|i| {
            let mut info: clap_audio_port_info = std::mem::zeroed();
            if (ext.get.unwrap())(plugin, i, true, &mut info) { info.channel_count } else { 2 }
        })
        .collect()
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

        if opts.list_params {
            let ext = (p.get_extension.unwrap())(plugin, CLAP_EXT_PARAMS.as_ptr()) as *const clap_plugin_params;
            check!(!ext.is_null(), "plugin has no params extension");
            for i in 0..((*ext).count.unwrap())(plugin) {
                let mut info: clap_param_info = std::mem::zeroed();
                if ((*ext).get_info.unwrap())(plugin, i, &mut info) {
                    println!("  {}", CStr::from_ptr(info.name.as_ptr()).to_string_lossy());
                }
            }
            (p.destroy.unwrap())(plugin);
            continue;
        }

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
        let block = opts.block;
        let blocks = opts.run_samples / block;
        let note_off = note.filter(|_| !opts.hold).map(|_| NOTE_OFF_SAMPLES);
        // A late note is measured from its start; otherwise wait for smoothing.
        let measure_from = if opts.note_at > 0 { opts.note_at } else { SETTLE_SAMPLES };
        let run_samples = opts.run_samples;
        let tempo = opts.tempo;
        if let Some(bpm) = tempo {
            println!("  transport: playing at {bpm} BPM from beat 0");
        }
        let input_channels = input_port_channels(plugin);
        if input_channels.len() > 1 {
            println!("  {} audio input ports (channels {:?}); extra ports get silence", input_channels.len(), input_channels);
        }
        let keys: Vec<u8> = match note {
            Some(key) => (0..opts.chord).map(|i| key as usize + 2 * i).filter(|&k| k <= 127).map(|k| k as u8).collect(),
            None => Vec::new(),
        };

        // Events per block, in time order: params and note-ons at the start,
        // then automation, and the note-offs.
        let mut schedule: Vec<Vec<Event>> = (0..blocks).map(|_| Vec::new()).collect();
        schedule[0] = param_events(plugin, &opts.sets)?;
        let last_sample = blocks * block - 1;
        let note_ons: Vec<usize> = match opts.note_every {
            Some(every) => (opts.note_at..=last_sample).step_by(every).collect(),
            None => vec![opts.note_at.min(last_sample)],
        };
        if let Some(every) = opts.note_every {
            println!("  note-ons every {every} samples ({} in all)", note_ons.len());
        }
        for &key in &keys {
            for &on in &note_ons {
                schedule[on / block].push(note_event(CLAP_EVENT_NOTE_ON, key, opts.velocity, (on % block) as u32));
            }
            if let Some(off) = note_off {
                schedule[off / block].push(note_event(CLAP_EVENT_NOTE_OFF, key, 0.0, (off % block) as u32));
            }
        }
        for (name, from, to) in &opts.automate {
            let (id, a) = resolve_param(plugin, name, from)?;
            let (_, b) = resolve_param(plugin, name, to)?;
            println!("  automate {name}: {from} .. {to} (values {a} .. {b}), every {} samples", opts.automate_every);
            for t in (0..blocks * block).step_by(opts.automate_every) {
                // Triangle 0 -> 1 -> 0 over one sweep.
                let x = (t % SWEEP_SAMPLES) as f64 / SWEEP_SAMPLES as f64;
                let tri = 1.0 - (2.0 * x - 1.0).abs();
                schedule[t / block].push(param_event(id, a + (b - a) * tri, (t % block) as u32));
            }
        }
        for events in &mut schedule {
            events.sort_by_key(|e| e.header().time);
        }
        if keys.len() > 1 {
            println!("  chord: {} notes from {}", keys.len(), keys[0]);
        }

        check!(
            (p.activate.unwrap())(plugin, SAMPLE_RATE, 1, block as u32),
            "activate(48k, {block}) returned false"
        );
        println!("  activate ok");

        // Processing happens on its own thread, like a real host's audio thread.
        let plugin_addr = plugin as usize;
        let tone = has_audio_input.then_some(opts.tone);
        let sidechain_kicks = opts.sidechain_kicks;
        if let Some(bpm) = sidechain_kicks {
            check!(input_channels.len() > 1, "--sidechain-kicks: the plugin has no sidechain (aux) input");
            println!("  sidechain: kicks at {bpm} BPM");
        }
        let m = std::thread::spawn(move || {
            process_blocks(
                plugin_addr as *const clap_plugin,
                schedule,
                block,
                tone,
                note_off,
                measure_from,
                run_samples,
                &input_channels,
                tempo,
                sidechain_kicks,
            )
        })
        .join()
        .map_err(|_| "audio thread panicked".to_string())??;

        let (left_dbfs, right_dbfs) = (to_dbfs(m.peak_l), to_dbfs(m.peak_r));
        let what = match (tone, note) {
            (Some(hz), Some(key)) => format!("a {hz} Hz tone + note {key}"),
            (Some(hz), None) => format!("a {hz} Hz tone"),
            (None, Some(key)) if keys.len() > 1 => format!("a {}-note chord from {key}", keys.len()),
            (None, Some(key)) => format!("note {key}"),
            (None, None) => "silence".to_string(),
        };

        if has_audio_input {
            let in_db = to_dbfs(INPUT_AMPLITUDE);
            let (left_db, right_db) = (left_dbfs - in_db, right_dbfs - in_db);
            let (exp_l, exp_r) = (opts.expect_left_db.unwrap_or(0.0), opts.expect_right_db.unwrap_or(0.0));
            println!(
                "  processed {blocks} blocks of {block} samples of {what}: left {left_db:+.3} dB, right {right_db:+.3} dB \
                 (expected {exp_l:+.3} / {exp_r:+.3} ± {})",
                opts.tol
            );
            check!(
                (left_db - exp_l).abs() <= opts.tol && (right_db - exp_r).abs() <= opts.tol,
                "output level outside tolerance"
            );
        } else {
            println!("  processed {blocks} blocks of {block} samples of {what}: peak left {left_dbfs:+.2} dBFS, right {right_dbfs:+.2} dBFS");
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

        if opts.trough_max_db.is_some() || opts.expect_trough_db.is_some() {
            let reference = if has_audio_input { to_dbfs(INPUT_AMPLITUDE) } else { 0.0 };
            let trough_db = to_dbfs(m.trough) - reference;
            println!("  trough (quietest 10 ms window): {trough_db:+.2} dB");
            if let Some(max) = opts.trough_max_db {
                check!(trough_db <= max, "trough {trough_db:+.2} dB is above {max:+.2} dB (not ducking enough?)");
            }
            if let Some(expect) = opts.expect_trough_db {
                check!((trough_db - expect).abs() <= opts.tol, "trough {trough_db:+.2} dB, expected {expect:+.2} ± {}", opts.tol);
            }
        }

        if note_off.is_some() {
            let tail_db = to_dbfs(m.tail_peak);
            println!("  after note-off: last {TAIL_SAMPLES} samples peak {tail_db:+.2} dBFS (must be < {SILENCE_DB} dBFS)");
            check!(tail_db < SILENCE_DB, "output did not decay to silence after note-off (stuck note?)");
        }

        let budget = block as f64 / SAMPLE_RATE;
        let load = |secs: f64| 100.0 * secs / budget;
        println!(
            "  process() load: mean {:.1} %, p99 {:.1} %, worst {:.1} % of the {:.2} ms budget",
            load(m.mean_secs),
            load(m.p99_secs),
            load(m.worst_secs),
            budget * 1e3
        );
        if let Some(max) = opts.max_load {
            check!(load(m.p99_secs) <= max, "p99 process() load {:.1} % is over {max} %", load(m.p99_secs));
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
    /// Peak over both channels in the last `TAIL_SAMPLES` samples.
    tail_peak: f32,
    /// The quietest 10 ms window's peak (both channels) in the measured range.
    trough: f32,
    /// Wall-clock time per `process()` call, skipping the first block
    /// (one-off setup).
    mean_secs: f64,
    p99_secs: f64,
    worst_secs: f64,
}

/// Run `BLOCKS` blocks through the plugin, sending `schedule[block]` with
/// each block. With `tone`, the input is a stereo sine (same on both
/// channels); without, the plugin gets no audio input.
/// A synthetic kick `t` seconds after it starts: a 60 Hz sine decaying
/// over 80 ms.
fn synthetic_kick(t: f64) -> f32 {
    (0.8 * (-t / 0.08).exp() * (2.0 * std::f64::consts::PI * 60.0 * t).sin()) as f32
}

unsafe fn process_blocks(
    plugin: *const clap_plugin,
    schedule: Vec<Vec<Event>>,
    block_len: usize,
    tone: Option<f32>,
    note_off: Option<usize>,
    measure_from: usize,
    run_samples: usize,
    input_channels: &[u32],
    tempo: Option<f64>,
    sidechain_kicks: Option<f64>,
) -> Result<Measurement, String> {
    const TROUGH_WINDOW: usize = 480; // 10 ms at 48 kHz
    let (mut window_peak, mut window_len) = (0f32, 0usize);
    let measure_end_sample = note_off.unwrap_or(run_samples);
    let blocks = schedule.len();
    let p = &*plugin;
    check!((p.start_processing.unwrap())(plugin), "start_processing returned false");

    let mut in_l = vec![0f32; block_len];
    let mut in_r = vec![0f32; block_len];
    let mut out_l = vec![0f32; block_len];
    let mut out_r = vec![0f32; block_len];
    let mut times = Vec::with_capacity(blocks);
    // Sidechain (aux) ports: silent buffers, one per channel.
    let mut aux_data: Vec<Vec<Vec<f32>>> = input_channels
        .iter()
        .skip(1)
        .map(|&n| (0..n).map(|_| vec![0f32; block_len]).collect())
        .collect();
    let mut aux_ptrs: Vec<Vec<*mut f32>> =
        aux_data.iter_mut().map(|chans| chans.iter_mut().map(|c| c.as_mut_ptr()).collect()).collect();
    let event_lists: Vec<clap_input_events> = schedule
        .iter()
        .map(|events| clap_input_events {
            ctx: events as *const _ as *mut c_void,
            size: Some(in_events_size),
            get: Some(in_events_get),
        })
        .collect();
    let out_events = clap_output_events { ctx: std::ptr::null_mut(), try_push: Some(out_events_push) };

    let settle_block = measure_from / block_len;
    let measure_end = note_off.unwrap_or(run_samples) / block_len;
    let tail_start = blocks - (TAIL_SAMPLES / block_len).max(1);
    let mut m = Measurement {
        peak_l: 0.0,
        peak_r: 0.0,
        tail_peak: 0.0,
        trough: f32::INFINITY,
        mean_secs: 0.0,
        p99_secs: 0.0,
        worst_secs: 0.0,
    };
    let mut phase = 0f64;
    let step = 2.0 * std::f64::consts::PI * tone.unwrap_or(0.0) as f64 / SAMPLE_RATE;
    for (block, in_events) in event_lists.iter().enumerate() {
        if tone.is_some() {
            for i in 0..block_len {
                let s = INPUT_AMPLITUDE * phase.sin() as f32;
                in_l[i] = s;
                in_r[i] = s;
                phase += step;
            }
        }
        if let Some(bpm) = sidechain_kicks {
            let period = (60.0 / bpm * SAMPLE_RATE).round() as usize;
            for chans in &mut aux_data {
                for i in 0..block_len {
                    let s = synthetic_kick(((block * block_len + i) % period) as f64 / SAMPLE_RATE);
                    for ch in chans.iter_mut() {
                        ch[i] = s;
                    }
                }
            }
        }
        let mut in_ptrs = [in_l.as_mut_ptr(), in_r.as_mut_ptr()];
        let mut out_ptrs = [out_l.as_mut_ptr(), out_r.as_mut_ptr()];
        let mut inputs = vec![clap_audio_buffer {
            data32: in_ptrs.as_mut_ptr(),
            data64: std::ptr::null_mut(),
            channel_count: 2,
            latency: 0,
            constant_mask: 0,
        }];
        for ptrs in &mut aux_ptrs {
            inputs.push(clap_audio_buffer {
                data32: ptrs.as_mut_ptr(),
                data64: std::ptr::null_mut(),
                channel_count: ptrs.len() as u32,
                latency: 0,
                constant_mask: 0,
            });
        }
        let mut outputs = clap_audio_buffer {
            data32: out_ptrs.as_mut_ptr(),
            data64: std::ptr::null_mut(),
            channel_count: 2,
            latency: 0,
            constant_mask: 0,
        };
        let transport = tempo.map(|bpm| {
            let beats = (block * block_len) as f64 * bpm / 60.0 / SAMPLE_RATE;
            clap_event_transport {
                header: clap_event_header {
                    size: std::mem::size_of::<clap_event_transport>() as u32,
                    time: 0,
                    space_id: CLAP_CORE_EVENT_SPACE_ID,
                    type_: CLAP_EVENT_TRANSPORT,
                    flags: 0,
                },
                flags: CLAP_TRANSPORT_HAS_TEMPO | CLAP_TRANSPORT_HAS_BEATS_TIMELINE | CLAP_TRANSPORT_IS_PLAYING,
                song_pos_beats: (beats * CLAP_BEATTIME_FACTOR as f64).round() as i64,
                song_pos_seconds: 0,
                tempo: bpm,
                tempo_inc: 0.0,
                loop_start_beats: 0,
                loop_end_beats: 0,
                loop_start_seconds: 0,
                loop_end_seconds: 0,
                bar_start: 0,
                bar_number: 0,
                tsig_num: 4,
                tsig_denom: 4,
            }
        });
        let process = clap_process {
            steady_time: (block * block_len) as i64,
            frames_count: block_len as u32,
            transport: transport.as_ref().map_or(std::ptr::null(), |t| t as *const _),
            audio_inputs: if tone.is_some() { inputs.as_ptr() } else { std::ptr::null() },
            audio_outputs: &mut outputs,
            audio_inputs_count: if tone.is_some() { inputs.len() as u32 } else { 0 },
            audio_outputs_count: 1,
            in_events,
            out_events: &out_events,
        };
        let start = Instant::now();
        let status = (p.process.unwrap())(plugin, &process);
        times.push(start.elapsed().as_secs_f64());
        check!(status != CLAP_PROCESS_ERROR, "process returned CLAP_PROCESS_ERROR on block {block}");
        for (&y, out) in out_l.iter().zip(&out_r) {
            check!(y.is_finite() && out.is_finite(), "non-finite output on block {block}");
        }
        for i in 0..block_len {
            let n = block * block_len + i;
            if n < measure_from || n >= measure_end_sample {
                continue;
            }
            window_peak = window_peak.max(out_l[i].abs()).max(out_r[i].abs());
            window_len += 1;
            if window_len == TROUGH_WINDOW {
                m.trough = m.trough.min(window_peak);
                (window_peak, window_len) = (0.0, 0);
            }
        }
        if (settle_block..measure_end).contains(&block) {
            m.peak_l = out_l.iter().fold(m.peak_l, |acc, s| acc.max(s.abs()));
            m.peak_r = out_r.iter().fold(m.peak_r, |acc, s| acc.max(s.abs()));
        }
        if block >= tail_start {
            m.tail_peak = out_l.iter().chain(&out_r).fold(m.tail_peak, |acc, s| acc.max(s.abs()));
        }
    }

    (p.stop_processing.unwrap())(plugin);

    let mut times = times.split_off(1.min(times.len()));
    times.sort_by(f64::total_cmp);
    if !times.is_empty() {
        m.mean_secs = times.iter().sum::<f64>() / times.len() as f64;
        m.p99_secs = times[(times.len() * 99 / 100).min(times.len() - 1)];
        m.worst_secs = times[times.len() - 1];
    }
    Ok(m)
}
