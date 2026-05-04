use std::process::Command;

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
    assert_eq!(json["audio"]["nonzero_samples"], 8);
    assert_eq!(json["audio"]["positive_peak_q15"], 32_767);
    assert_eq!(json["audio"]["negative_peak_q15"], 32_767);
    assert_eq!(json["midi"].as_array().unwrap().len(), 1);
}
