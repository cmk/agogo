use std::process::Command;

use agogo::chan::channel::Channel;
use agogo::chan::channel::spec::parse_channels;
use agogo::chan::conn::tempo::Tempo;
use agogo::core::{OfflineRenderConfig, render_offline_capture};

#[test]
fn render_outputs_deterministic_json() {
    let output = Command::new(env!("CARGO_BIN_EXE_agogo"))
        .args([
            "render",
            "--source",
            "internal",
            "--bpm",
            "120",
            "--sr",
            "48000",
            "--duration-bars",
            "1",
            "--buffer-frames",
            "4096",
            "--ch",
            "id=three,dev=midi,mode=clock,grid=t2t,out=diag",
            "--ch",
            "id=two,dev=midi,mode=clock,grid=t2,out=diag",
        ])
        .output()
        .expect("run agogo render");

    assert!(
        output.status.success(),
        "agogo render failed: status={:?}, stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8(output.stdout).expect("render stdout is utf8");
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("render stdout is JSON");
    assert_eq!(json["source"], "internal");
    assert_eq!(json["sample_rate"], 48_000);
    assert_eq!(json["duration"]["frames"], 96_000);
    assert_eq!(json["dropped"], 0);
    assert_eq!(json["midi"][0]["sample"], 0);
    assert_eq!(json["midi"][0]["bytes"], serde_json::json!([250]));
}

#[test]
fn render_rejects_non_internal_source() {
    let output = Command::new(env!("CARGO_BIN_EXE_agogo"))
        .args([
            "render",
            "--source",
            "link",
            "--bpm",
            "120",
            "--ch",
            "dev=midi,mode=clock,grid=t4,out=diag",
        ])
        .output()
        .expect("run agogo render");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unsupported for offline render"),
        "{stderr}"
    );
}

#[test]
fn render_rejects_real_output_targets() {
    let output = Command::new(env!("CARGO_BIN_EXE_agogo"))
        .args([
            "render",
            "--source",
            "internal",
            "--bpm",
            "120",
            "--ch",
            "dev=midi,mode=clock,grid=t4,out=hw-port",
        ])
        .output()
        .expect("run agogo render");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unsupported for offline render") && stderr.contains("out=diag"),
        "{stderr}"
    );
}

#[test]
fn render_outputs_cv_pulse_audio_summary() {
    let output = Command::new(env!("CARGO_BIN_EXE_agogo"))
        .args([
            "render",
            "--source",
            "internal",
            "--bpm",
            "120",
            "--sr",
            "48000",
            "--duration-bars",
            "1",
            "--buffer-frames",
            "4096",
            "--ch",
            "id=cv,dev=cv,mode=pulse,grid=t4,out=diag",
        ])
        .output()
        .expect("run agogo render");

    assert!(
        output.status.success(),
        "agogo render failed: status={:?}, stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8(output.stdout).expect("render stdout is utf8");
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("render stdout is JSON");
    // CV pulse writes to every output lane (dual-mono); the offline
    // render path now defaults to --output-channels=2, so the
    // four bipolar pulses (two samples each = 8 nonzero per lane)
    // tally to 16 across both lanes. Plan 2026-05-05-02 T1.
    assert_eq!(json["audio"]["nonzero_samples"], 16);
    assert_eq!(json["audio"]["positive_peak_q15"], 32_767);
    assert_eq!(json["audio"]["negative_peak_q15"], 32_767);
    assert_eq!(json["midi"].as_array().unwrap().len(), 1);
}

/// CLI parity: `agogo render` with `--output-channels 4` and four
/// audio click channels routed to lanes 0..3 produces JSON
/// aggregates that agree with the per-lane PCM the in-process
/// `render_offline_capture` returns from the equivalent config.
/// Single deterministic case — confirms the CLI is using the
/// same code path and that the multi-channel offline render works
/// end-to-end through the binary. Plan 2026-05-05-02 T6.
#[test]
fn render_4_channel_cli_matches_library_aggregates() {
    let ch_specs = [
        "id=ch1,dev=audio,mode=click,grid=t2t,out=0",
        "id=ch2,dev=audio,mode=click,grid=t2,out=1",
        "id=ch3,dev=audio,mode=click,grid=t4,out=2",
        "id=ch4,dev=audio,mode=click,grid=t4t,out=3",
    ];
    let mut cli_args: Vec<&str> = vec![
        "render",
        "--source",
        "internal",
        "--bpm",
        "120",
        "--sr",
        "48000",
        "--duration-bars",
        "1",
        "--buffer-frames",
        "1024",
        "--output-channels",
        "4",
    ];
    for spec in &ch_specs {
        cli_args.push("--ch");
        cli_args.push(spec);
    }
    let cli_output = Command::new(env!("CARGO_BIN_EXE_agogo"))
        .args(&cli_args)
        .output()
        .expect("run agogo render");
    assert!(
        cli_output.status.success(),
        "agogo render failed: status={:?}, stderr={}",
        cli_output.status.code(),
        String::from_utf8_lossy(&cli_output.stderr),
    );
    let stdout = String::from_utf8(cli_output.stdout).expect("render stdout is utf8");
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("render stdout is JSON");

    // Drive the same render in-process and re-derive aggregates
    // from the captured PCM. Both sides should agree exactly.
    let specs: Vec<String> = ch_specs.iter().map(|s| (*s).to_string()).collect();
    let named = parse_channels(&specs).expect("parse channels");
    let channels: Vec<Channel> = named
        .into_iter()
        .map(|(_, spec)| spec.into_channel().expect("into_channel"))
        .collect();
    let bpm = Tempo::from_bpm_integer(120);
    let sr = 48_000_u32;
    let total_frames = json["duration"]["frames"]
        .as_u64()
        .expect("duration.frames");
    let cfg = OfflineRenderConfig {
        channels,
        bpm,
        sample_rate: sr,
        buffer_frames: 1024,
        total_frames,
        output_channels: 4,
    };
    let (report, pcm) = render_offline_capture(cfg).expect("render_offline_capture");

    // Aggregates from the captured PCM, computed identically to
    // render_offline's per-buffer tally.
    let mut nonzero = 0_u64;
    let mut peak_q15 = 0_u16;
    let mut pos_peak_q15 = 0_u16;
    let mut neg_peak_q15 = 0_u16;
    for lane in &pcm.lanes {
        for &s in lane {
            if s != 0.0 {
                nonzero += 1;
            }
            let q15 = (s.abs().min(1.0) * 32767.0_f32).round() as u16; // PCM ABI
            peak_q15 = peak_q15.max(q15);
            if s > 0.0 {
                pos_peak_q15 = pos_peak_q15.max(q15);
            }
            if s < 0.0 {
                neg_peak_q15 = neg_peak_q15.max(q15);
            }
        }
    }

    assert_eq!(report.audio_nonzero_samples, nonzero);
    assert_eq!(json["audio"]["nonzero_samples"].as_u64(), Some(nonzero));
    assert_eq!(json["audio"]["peak_q15"].as_u64(), Some(peak_q15.into()));
    assert_eq!(
        json["audio"]["positive_peak_q15"].as_u64(),
        Some(pos_peak_q15.into())
    );
    assert_eq!(
        json["audio"]["negative_peak_q15"].as_u64(),
        Some(neg_peak_q15.into())
    );
    // 4-lane render means lanes.len() == 4 and every lane has
    // total_frames samples.
    assert_eq!(pcm.lanes.len(), 4);
    for lane in &pcm.lanes {
        assert_eq!(lane.len() as u64, total_frames);
    }
    // Every lane should have at least one nonzero sample (the
    // tick-0 click hit on every audio channel).
    for (i, lane) in pcm.lanes.iter().enumerate() {
        assert!(
            lane.iter().any(|&s| s != 0.0),
            "lane {i} expected at least one nonzero sample"
        );
    }
}
