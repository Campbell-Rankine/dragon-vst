//! Headless CLAP smoke test: load a `.clap` bundle, instantiate every plugin in
//! it, run audio through it, and save its state. Exits non-zero on any failure.
//!
//! Usage: cargo run -p clap-smoke -- "target/bundled/Dragon ThreeQ.clap"

use clap_sys::audio_buffer::clap_audio_buffer;
use clap_sys::entry::clap_plugin_entry;
use clap_sys::events::{clap_event_header, clap_input_events, clap_output_events};
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

macro_rules! check {
    ($cond:expr, $($msg:tt)*) => {
        if !$cond {
            return Err(format!($($msg)*));
        }
    };
}

unsafe extern "C" fn host_get_extension(_: *const clap_host, _: *const c_char) -> *const c_void {
    std::ptr::null()
}
unsafe extern "C" fn host_noop(_: *const clap_host) {}

unsafe extern "C" fn in_events_size(_: *const clap_input_events) -> u32 {
    0
}
unsafe extern "C" fn in_events_get(_: *const clap_input_events, _: u32) -> *const clap_event_header {
    std::ptr::null()
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
    let Some(bundle) = std::env::args().nth(1).map(PathBuf::from) else {
        eprintln!("usage: clap-smoke <path/to/Plugin.clap>");
        return ExitCode::from(2);
    };
    match unsafe { run(&bundle) } {
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

unsafe fn run(bundle: &PathBuf) -> Result<(), String> {
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

        check!(
            (p.activate.unwrap())(plugin, SAMPLE_RATE, 1, BLOCK as u32),
            "activate(48k, 512) returned false"
        );
        println!("  activate ok");

        // Processing happens on its own thread, like a real host's audio thread.
        let plugin_addr = plugin as usize;
        let peak = std::thread::spawn(move || process_blocks(plugin_addr as *const clap_plugin))
            .join()
            .map_err(|_| "audio thread panicked".to_string())??;
        println!("  processed {BLOCKS} blocks, output peak {peak:.4} (input peak 0.5000)");
        check!((peak - 0.5).abs() < 1e-3, "unity-gain passthrough expected, got peak {peak}");

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

/// Feed a 0.5-amplitude stereo sine through the plugin; return the output peak.
unsafe fn process_blocks(plugin: *const clap_plugin) -> Result<f32, String> {
    let p = &*plugin;
    check!((p.start_processing.unwrap())(plugin), "start_processing returned false");

    let mut in_l = vec![0f32; BLOCK];
    let mut in_r = vec![0f32; BLOCK];
    let mut out_l = vec![0f32; BLOCK];
    let mut out_r = vec![0f32; BLOCK];
    let in_events = clap_input_events { ctx: std::ptr::null_mut(), size: Some(in_events_size), get: Some(in_events_get) };
    let out_events = clap_output_events { ctx: std::ptr::null_mut(), try_push: Some(out_events_push) };

    let mut peak = 0f32;
    let mut phase = 0f32;
    for block in 0..BLOCKS {
        for i in 0..BLOCK {
            let s = 0.5 * phase.sin();
            in_l[i] = s;
            in_r[i] = s;
            phase += 2.0 * std::f32::consts::PI * 1000.0 / SAMPLE_RATE as f32;
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
            in_events: &in_events,
            out_events: &out_events,
        };
        let status = (p.process.unwrap())(plugin, &process);
        check!(status != CLAP_PROCESS_ERROR, "process returned CLAP_PROCESS_ERROR on block {block}");
        // Skip the first blocks so parameter smoothing has settled.
        if block > 10 {
            peak = out_l.iter().chain(&out_r).fold(peak, |m, s| m.max(s.abs()));
        }
    }

    (p.stop_processing.unwrap())(plugin);
    Ok(peak)
}
