//! Headless CLAP smoke test: load a `.clap` bundle, instantiate every plugin in
//! it, run a stereo sine through it, check the output level, and save its
//! state. Exits non-zero on any failure.
//!
//! ```text
//! clap-smoke <Plugin.clap> [options]
//!
//!   --set "<param name>=<text>"   set a parameter before processing (repeatable);
//!                                 the text is parsed by the plugin, e.g. "12",
//!                                 "Low Cut", "On" / "Off", "48 dB/oct"
//!   --tone <Hz>                   test tone frequency (default 1000)
//!   --expect-db <dB>              expected output level change, both channels (default 0)
//!   --expect-left-db <dB>         override for the left channel
//!   --expect-right-db <dB>        override for the right channel
//!   --tol <dB>                    allowed error (default 0.05)
//! ```
//!
//! Examples:
//! ```text
//! cargo run -p clap-smoke -- "target/bundled/Dragon 24EQ.clap"
//! cargo run -p clap-smoke -- "target/bundled/Dragon 24EQ.clap" --set "Band 2 Gain=12" --expect-db 12
//! ```

use clap_sys::audio_buffer::clap_audio_buffer;
use clap_sys::entry::clap_plugin_entry;
use clap_sys::events::{
    clap_event_header, clap_event_param_value, clap_input_events, clap_output_events, CLAP_CORE_EVENT_SPACE_ID,
    CLAP_EVENT_PARAM_VALUE,
};
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
    expect_left_db: f64,
    expect_right_db: f64,
    tol: f64,
}

fn parse_args() -> Result<Options, String> {
    let mut args = std::env::args().skip(1);
    let bundle = PathBuf::from(args.next().ok_or("missing <Plugin.clap>")?);
    let mut opts =
        Options { bundle, sets: Vec::new(), tone: 1000.0, expect_left_db: 0.0, expect_right_db: 0.0, tol: 0.05 };
    let (mut left, mut right, mut both) = (None, None, 0.0);
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
            "--expect-db" => both = num(value()?)?,
            "--expect-left-db" => left = Some(num(value()?)?),
            "--expect-right-db" => right = Some(num(value()?)?),
            "--tol" => opts.tol = num(value()?)?,
            other => return Err(format!("unknown option {other}")),
        }
    }
    opts.expect_left_db = left.unwrap_or(both);
    opts.expect_right_db = right.unwrap_or(both);
    Ok(opts)
}

unsafe extern "C" fn host_get_extension(_: *const clap_host, _: *const c_char) -> *const c_void {
    std::ptr::null()
}
unsafe extern "C" fn host_noop(_: *const clap_host) {}

/// Input event list backed by a `Vec<clap_event_param_value>` in `ctx`.
unsafe extern "C" fn in_events_size(list: *const clap_input_events) -> u32 {
    let events = &*((*list).ctx as *const Vec<clap_event_param_value>);
    events.len() as u32
}
unsafe extern "C" fn in_events_get(list: *const clap_input_events, index: u32) -> *const clap_event_header {
    let events = &*((*list).ctx as *const Vec<clap_event_param_value>);
    &events[index as usize].header
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
            eprintln!("error: {e}\nusage: clap-smoke <Plugin.clap> [--set NAME=TEXT]... [--tone HZ] [--expect-db DB] [--expect-left-db DB] [--expect-right-db DB] [--tol DB]");
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
unsafe fn param_events(plugin: *const clap_plugin, sets: &[(String, String)]) -> Result<Vec<clap_event_param_value>, String> {
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
        events.push(clap_event_param_value {
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
        });
    }
    Ok(events)
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

        let events = param_events(plugin, &opts.sets)?;

        check!(
            (p.activate.unwrap())(plugin, SAMPLE_RATE, 1, BLOCK as u32),
            "activate(48k, 512) returned false"
        );
        println!("  activate ok");

        // Processing happens on its own thread, like a real host's audio thread.
        let plugin_addr = plugin as usize;
        let tone = opts.tone;
        let (left, right) = std::thread::spawn(move || process_blocks(plugin_addr as *const clap_plugin, events, tone))
            .join()
            .map_err(|_| "audio thread panicked".to_string())??;
        let to_db = |peak: f32| 20.0 * (peak as f64 / INPUT_AMPLITUDE as f64).log10();
        let (left_db, right_db) = (to_db(left), to_db(right));
        println!(
            "  processed {BLOCKS} blocks of a {tone} Hz tone: left {left_db:+.3} dB, right {right_db:+.3} dB \
             (expected {:+.3} / {:+.3} ± {})",
            opts.expect_left_db, opts.expect_right_db, opts.tol
        );
        check!(
            (left_db - opts.expect_left_db).abs() <= opts.tol && (right_db - opts.expect_right_db).abs() <= opts.tol,
            "output level outside tolerance"
        );

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

/// Feed a stereo sine (same on both channels) through the plugin, sending
/// `events` with the first block. Returns the settled output peak per channel.
unsafe fn process_blocks(
    plugin: *const clap_plugin,
    events: Vec<clap_event_param_value>,
    tone: f32,
) -> Result<(f32, f32), String> {
    let p = &*plugin;
    check!((p.start_processing.unwrap())(plugin), "start_processing returned false");

    let mut in_l = vec![0f32; BLOCK];
    let mut in_r = vec![0f32; BLOCK];
    let mut out_l = vec![0f32; BLOCK];
    let mut out_r = vec![0f32; BLOCK];
    let no_events: Vec<clap_event_param_value> = Vec::new();
    let first_events =
        clap_input_events { ctx: &events as *const _ as *mut c_void, size: Some(in_events_size), get: Some(in_events_get) };
    let later_events =
        clap_input_events { ctx: &no_events as *const _ as *mut c_void, size: Some(in_events_size), get: Some(in_events_get) };
    let out_events = clap_output_events { ctx: std::ptr::null_mut(), try_push: Some(out_events_push) };

    let (mut peak_l, mut peak_r) = (0f32, 0f32);
    let mut phase = 0f64;
    let step = 2.0 * std::f64::consts::PI * tone as f64 / SAMPLE_RATE;
    for block in 0..BLOCKS {
        for i in 0..BLOCK {
            let s = INPUT_AMPLITUDE * phase.sin() as f32;
            in_l[i] = s;
            in_r[i] = s;
            phase += step;
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
            audio_inputs: &inputs,
            audio_outputs: &mut outputs,
            audio_inputs_count: 1,
            audio_outputs_count: 1,
            in_events: if block == 0 { &first_events } else { &later_events },
            out_events: &out_events,
        };
        let status = (p.process.unwrap())(plugin, &process);
        check!(status != CLAP_PROCESS_ERROR, "process returned CLAP_PROCESS_ERROR on block {block}");
        for (&y, out) in out_l.iter().zip(&out_r) {
            check!(y.is_finite() && out.is_finite(), "non-finite output on block {block}");
        }
        if block >= SETTLE_BLOCKS {
            peak_l = out_l.iter().fold(peak_l, |m, s| m.max(s.abs()));
            peak_r = out_r.iter().fold(peak_r, |m, s| m.max(s.abs()));
        }
    }

    (p.stop_processing.unwrap())(plugin);
    Ok((peak_l, peak_r))
}
