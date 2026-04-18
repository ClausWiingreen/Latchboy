use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::process::{self, Command};
use std::time::Instant;

use latchboy_core::{cartridge::Cartridge, Emulator};
use latchboy_desktop::{run_emulation_loop, FramePresenter};

const DEFAULT_CYCLE_STEP: u32 = 1_024;

#[derive(Clone, Copy)]
struct MatrixPreset {
    title_id: &'static str,
    frame_limit: u64,
    wall_time_limit_ms: u64,
    checkpoint_start_frame: u64,
    checkpoint_frame_count: u64,
}

const MATRIX_PRESETS: [MatrixPreset; 3] = [
    MatrixPreset {
        title_id: "tetris-world",
        frame_limit: 420,
        wall_time_limit_ms: 10_000,
        checkpoint_start_frame: 300,
        checkpoint_frame_count: 120,
    },
    MatrixPreset {
        title_id: "super-mario-land-world",
        frame_limit: 540,
        wall_time_limit_ms: 12_000,
        checkpoint_start_frame: 420,
        checkpoint_frame_count: 120,
    },
    MatrixPreset {
        title_id: "legend-of-zelda-links-awakening-world",
        frame_limit: 720,
        wall_time_limit_ms: 15_000,
        checkpoint_start_frame: 600,
        checkpoint_frame_count: 120,
    },
];

#[derive(Debug)]
struct UsageError(String);

fn known_title_ids() -> String {
    MATRIX_PRESETS
        .iter()
        .map(|preset| preset.title_id)
        .collect::<Vec<_>>()
        .join(", ")
}

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for UsageError {}

#[derive(Debug)]
struct CliConfig {
    rom_path: PathBuf,
    rom_id: String,
    title_id: Option<String>,
    output_dir: PathBuf,
    runner_command: String,
    frame_limit: u64,
    wall_time_limit_ms: u64,
    checkpoint_start_frame: u64,
    checkpoint_frame_count: u64,
    title_signal_frame: Option<u64>,
    title_signal_hash: Option<String>,
    hash_start_frame: u64,
    hash_frame_count: u64,
    hash_sample_stride: u64,
    cycle_step: u32,
}

#[derive(Clone, Debug)]
struct SampledFrameHash {
    frame_index: u64,
    hash: String,
}

#[derive(Debug)]
struct SmokePresenter {
    started: Instant,
    wall_time_limit_ms: u64,
    frame_limit: u64,
    checkpoint_start_frame: u64,
    checkpoint_frame_count: u64,
    hash_start_frame: u64,
    hash_end_exclusive: u64,
    hash_sample_stride: u64,
    frames_presented: u64,
    sampled_hashes: Vec<SampledFrameHash>,
    first_presented_hash: Option<SampledFrameHash>,
    timed_out: bool,
}

impl SmokePresenter {
    fn new(config: &CliConfig) -> Self {
        let hash_end_exclusive = config
            .hash_start_frame
            .saturating_add(config.hash_frame_count.max(1));

        Self {
            started: Instant::now(),
            wall_time_limit_ms: config.wall_time_limit_ms,
            frame_limit: config.frame_limit,
            checkpoint_start_frame: config.checkpoint_start_frame,
            checkpoint_frame_count: config.checkpoint_frame_count.max(1),
            hash_start_frame: config.hash_start_frame,
            hash_end_exclusive,
            hash_sample_stride: config.hash_sample_stride.max(1),
            frames_presented: 0,
            sampled_hashes: Vec::new(),
            first_presented_hash: None,
            timed_out: false,
        }
    }

    fn elapsed_ms(&self) -> u128 {
        self.started.elapsed().as_millis()
    }

    fn checkpoint_reached(&self) -> bool {
        let required_end = self
            .checkpoint_start_frame
            .saturating_add(self.checkpoint_frame_count);
        self.frames_presented >= required_end
    }
}

impl FramePresenter for SmokePresenter {
    type Error = std::io::Error;

    fn is_open(&self) -> bool {
        if self.frames_presented >= self.frame_limit {
            return false;
        }
        self.elapsed_ms() <= u128::from(self.wall_time_limit_ms)
    }

    fn present_frame(&mut self, surface: &[u32]) -> Result<(), Self::Error> {
        let frame_index = self.frames_presented;
        let capture_hash_sample = frame_index >= self.hash_start_frame
            && frame_index < self.hash_end_exclusive
            && (frame_index - self.hash_start_frame).is_multiple_of(self.hash_sample_stride);
        let capture_fallback_sample = self.first_presented_hash.is_none();
        if capture_hash_sample || capture_fallback_sample {
            let hash = fnv1a64_surface_hash(surface);
            let sampled_frame = SampledFrameHash {
                frame_index,
                hash: format!("0x{hash:016x}"),
            };
            if capture_fallback_sample {
                self.first_presented_hash = Some(sampled_frame.clone());
            }
            if capture_hash_sample {
                self.sampled_hashes.push(sampled_frame);
            }
        }

        self.frames_presented = self.frames_presented.saturating_add(1);
        if self.elapsed_ms() > u128::from(self.wall_time_limit_ms) {
            self.timed_out = true;
        }
        Ok(())
    }
}

fn fnv1a64_surface_hash(surface: &[u32]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for pixel in surface {
        for byte in pixel.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

fn parse_u64(value: &str, name: &str) -> Result<u64, UsageError> {
    value.parse::<u64>().map_err(|_| {
        UsageError(format!(
            "invalid --{name} value '{value}': expected integer"
        ))
    })
}

fn parse_u32(value: &str, name: &str) -> Result<u32, UsageError> {
    value.parse::<u32>().map_err(|_| {
        UsageError(format!(
            "invalid --{name} value '{value}': expected integer"
        ))
    })
}

fn normalize_hash(value: &str) -> String {
    let trimmed = value.trim();
    let without_prefix = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    without_prefix.to_ascii_lowercase()
}

fn hash_window_end_exclusive(hash_start_frame: u64, hash_frame_count: u64) -> u64 {
    hash_start_frame.saturating_add(hash_frame_count.max(1))
}

fn frame_is_hash_sample(
    frame_index: u64,
    hash_start_frame: u64,
    hash_frame_count: u64,
    hash_sample_stride: u64,
) -> bool {
    let hash_end_exclusive = hash_window_end_exclusive(hash_start_frame, hash_frame_count);
    frame_index >= hash_start_frame
        && frame_index < hash_end_exclusive
        && (frame_index - hash_start_frame).is_multiple_of(hash_sample_stride.max(1))
}

fn default_title_signal_frame(
    checkpoint_frame_index: u64,
    hash_start_frame: u64,
    hash_frame_count: u64,
    hash_sample_stride: u64,
) -> u64 {
    let hash_end_exclusive = hash_window_end_exclusive(hash_start_frame, hash_frame_count);
    let max_in_window = checkpoint_frame_index.min(hash_end_exclusive.saturating_sub(1));
    if max_in_window < hash_start_frame {
        return hash_start_frame;
    }

    let distance = max_in_window - hash_start_frame;
    let stride = hash_sample_stride.max(1);
    let offset = distance - (distance % stride);
    hash_start_frame.saturating_add(offset)
}

fn shell_escape_arg(value: &str) -> String {
    if value.is_empty() {
        return "''".to_owned();
    }

    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || "-_./:=".contains(ch))
    {
        return value.to_owned();
    }

    let escaped = value.replace('\'', "'\"'\"'");
    format!("'{escaped}'")
}

fn parse_args() -> Result<CliConfig, UsageError> {
    let provided_args = env::args().skip(1).collect::<Vec<_>>();
    let mut args = provided_args.iter();

    let mut rom_path: Option<PathBuf> = None;
    let mut rom_id: Option<String> = None;
    let mut title_id: Option<String> = None;
    let mut output_dir: Option<PathBuf> = None;

    let mut frame_limit: Option<u64> = None;
    let mut wall_time_limit_ms: Option<u64> = None;
    let mut checkpoint_start_frame: Option<u64> = None;
    let mut checkpoint_frame_count: Option<u64> = None;
    let mut title_signal_frame: Option<u64> = None;
    let mut title_signal_hash: Option<String> = None;
    let mut hash_start_frame: Option<u64> = None;
    let mut hash_frame_count: Option<u64> = None;
    let mut hash_sample_stride: Option<u64> = None;
    let mut cycle_step: Option<u32> = None;

    while let Some(flag) = args.next() {
        if matches!(flag.as_str(), "--help" | "-h") {
            return Err(UsageError(help_text()));
        }

        let value = args
            .next()
            .ok_or_else(|| UsageError(format!("missing value for argument '{flag}'")))?;

        match flag.as_str() {
            "--rom" => rom_path = Some(PathBuf::from(value)),
            "--rom-id" => rom_id = Some(value.clone()),
            "--title-id" => title_id = Some(value.clone()),
            "--output-dir" => output_dir = Some(PathBuf::from(value)),
            "--frame-limit" => frame_limit = Some(parse_u64(value, "frame-limit")?),
            "--wall-time-limit-ms" => {
                wall_time_limit_ms = Some(parse_u64(value, "wall-time-limit-ms")?)
            }
            "--checkpoint-start-frame" => {
                checkpoint_start_frame = Some(parse_u64(value, "checkpoint-start-frame")?)
            }
            "--checkpoint-frame-count" => {
                checkpoint_frame_count = Some(parse_u64(value, "checkpoint-frame-count")?)
            }
            "--title-signal-frame" => {
                title_signal_frame = Some(parse_u64(value, "title-signal-frame")?)
            }
            "--title-signal-hash" => title_signal_hash = Some(normalize_hash(value)),
            "--hash-start-frame" => hash_start_frame = Some(parse_u64(value, "hash-start-frame")?),
            "--hash-frame-count" => hash_frame_count = Some(parse_u64(value, "hash-frame-count")?),
            "--hash-sample-stride" => {
                hash_sample_stride = Some(parse_u64(value, "hash-sample-stride")?)
            }
            "--cycle-step" => cycle_step = Some(parse_u32(value, "cycle-step")?),
            _ => {
                return Err(UsageError(format!(
                    "unknown argument '{flag}'\n\n{}",
                    help_text()
                )));
            }
        }
    }

    let rom_path =
        rom_path.ok_or_else(|| UsageError(format!("missing required --rom\n\n{}", help_text())))?;
    let output_dir = output_dir
        .ok_or_else(|| UsageError(format!("missing required --output-dir\n\n{}", help_text())))?;

    let selected_preset = match title_id.as_deref() {
        Some(id) => {
            let preset = MATRIX_PRESETS
                .iter()
                .find(|preset| preset.title_id == id)
                .copied();
            if preset.is_none() {
                return Err(UsageError(format!(
                    "unknown --title-id '{id}'. Expected one of: {}",
                    known_title_ids()
                )));
            }
            preset
        }
        None => None,
    };

    let rom_id = rom_id.unwrap_or_else(|| {
        rom_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("unknown-rom")
            .to_owned()
    });

    let frame_limit = frame_limit
        .or(selected_preset.map(|preset| preset.frame_limit))
        .unwrap_or(300);
    let wall_time_limit_ms = wall_time_limit_ms
        .or(selected_preset.map(|preset| preset.wall_time_limit_ms))
        .unwrap_or(10_000);
    let checkpoint_start_frame = checkpoint_start_frame
        .or(selected_preset.map(|preset| preset.checkpoint_start_frame))
        .unwrap_or(frame_limit.saturating_sub(120));
    let checkpoint_frame_count = checkpoint_frame_count
        .or(selected_preset.map(|preset| preset.checkpoint_frame_count))
        .unwrap_or(120);
    let checkpoint_frame_index = checkpoint_start_frame
        .saturating_add(checkpoint_frame_count)
        .saturating_sub(1);

    let hash_start_frame = hash_start_frame.unwrap_or(checkpoint_start_frame);
    let hash_frame_count = hash_frame_count.unwrap_or(checkpoint_frame_count);
    let hash_sample_stride = hash_sample_stride.unwrap_or(1);
    let title_signal_frame = title_signal_frame.or_else(|| {
        title_id.as_ref().map(|_| {
            default_title_signal_frame(
                checkpoint_frame_index,
                hash_start_frame,
                hash_frame_count,
                hash_sample_stride,
            )
        })
    });
    let cycle_step = cycle_step.unwrap_or(DEFAULT_CYCLE_STEP);
    let runner_command = format!(
        "cargo run -p latchboy-desktop --bin milestone4_smoke -- {}",
        provided_args
            .iter()
            .map(|arg| shell_escape_arg(arg))
            .collect::<Vec<_>>()
            .join(" ")
    );

    if frame_limit == 0 {
        return Err(UsageError(
            "--frame-limit must be greater than zero".to_owned(),
        ));
    }
    if wall_time_limit_ms == 0 {
        return Err(UsageError(
            "--wall-time-limit-ms must be greater than zero".to_owned(),
        ));
    }
    if checkpoint_frame_count == 0 {
        return Err(UsageError(
            "--checkpoint-frame-count must be greater than zero".to_owned(),
        ));
    }
    if hash_frame_count == 0 {
        return Err(UsageError(
            "--hash-frame-count must be greater than zero".to_owned(),
        ));
    }
    if hash_sample_stride == 0 {
        return Err(UsageError(
            "--hash-sample-stride must be greater than zero".to_owned(),
        ));
    }
    if cycle_step == 0 {
        return Err(UsageError(
            "--cycle-step must be greater than zero".to_owned(),
        ));
    }
    if title_id.is_some() && title_signal_hash.is_none() {
        return Err(UsageError(
            "--title-id requires --title-signal-hash so PASS can be gated on title-specific signal evidence".to_owned(),
        ));
    }
    if let Some(frame) = title_signal_frame {
        if !frame_is_hash_sample(
            frame,
            hash_start_frame,
            hash_frame_count,
            hash_sample_stride,
        ) {
            return Err(UsageError(format!(
                "--title-signal-frame {} is not sampled by hash window start={} frame_count={} stride={}",
                frame, hash_start_frame, hash_frame_count, hash_sample_stride
            )));
        }
    }

    Ok(CliConfig {
        rom_path,
        rom_id,
        title_id,
        output_dir,
        runner_command,
        frame_limit,
        wall_time_limit_ms,
        checkpoint_start_frame,
        checkpoint_frame_count,
        title_signal_frame,
        title_signal_hash,
        hash_start_frame,
        hash_frame_count,
        hash_sample_stride,
        cycle_step,
    })
}

fn help_text() -> String {
    let mut preset_lines = String::new();
    for preset in MATRIX_PRESETS {
        preset_lines.push_str(&format!(
            "  - {} (frame_limit={}, wall_time_limit_ms={}, checkpoint={}..{})\n",
            preset.title_id,
            preset.frame_limit,
            preset.wall_time_limit_ms,
            preset.checkpoint_start_frame,
            preset
                .checkpoint_start_frame
                .saturating_add(preset.checkpoint_frame_count)
                .saturating_sub(1)
        ));
    }

    format!(
        "milestone4_smoke usage:\n\
         cargo run -p latchboy-desktop --bin milestone4_smoke -- \\\n           --rom <path/to/game.gb> \\\n           --output-dir <tests/artifacts/smoke/milestone4/<timestamp>/<title_id>> \\\n           [--rom-id <stable-rom-id>] [--title-id <preset>]\n\n\
         Optional overrides:\n\
           --frame-limit <u64>\n\
           --wall-time-limit-ms <u64>\n\
           --checkpoint-start-frame <u64>\n\
           --checkpoint-frame-count <u64>\n\
           --title-signal-frame <u64>\n\
           --title-signal-hash <hex|0xhex>  # required when --title-id is set\n\
           --hash-start-frame <u64>\n\
           --hash-frame-count <u64>\n\
           --hash-sample-stride <u64>\n\
           --cycle-step <u32>\n\n\
         Built-in Milestone 4 matrix presets:\n{}",
        preset_lines
    )
}

fn git_commit_sha() -> Result<String, Box<dyn Error>> {
    let output = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .map_err(|error| format!("failed to execute git rev-parse: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(format!("git rev-parse --short=12 HEAD failed: {stderr}").into());
    }

    let sha = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let valid_sha = !sha.is_empty() && sha.chars().all(|ch| ch.is_ascii_hexdigit());
    if !valid_sha {
        return Err(format!(
            "git rev-parse returned non-hex commit SHA '{sha}', cannot emit schema-compatible run.json"
        )
        .into());
    }

    Ok(sha.to_ascii_lowercase())
}

fn escape_json(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn json_object(fields: &BTreeMap<&str, String>) -> String {
    let mut lines = Vec::with_capacity(fields.len() + 2);
    lines.push("{".to_owned());

    for (index, (key, value)) in fields.iter().enumerate() {
        let comma = if index + 1 == fields.len() { "" } else { "," };
        lines.push(format!("  \"{}\": {}{}", key, value, comma));
    }

    lines.push("}".to_owned());
    lines.join("\n")
}

fn quoted(value: &str) -> String {
    format!("\"{}\"", escape_json(value))
}

fn expected_hash_sample_count(config: &CliConfig) -> u64 {
    config
        .hash_frame_count
        .saturating_sub(1)
        .saturating_div(config.hash_sample_stride)
        .saturating_add(1)
}

fn title_signal_matches(config: &CliConfig, presenter: &SmokePresenter) -> Result<bool, String> {
    let expected_hash = match config.title_signal_hash.as_deref() {
        Some(value) => value,
        None => return Ok(true),
    };
    let signal_frame = config.title_signal_frame.unwrap_or(0);

    let observed = presenter
        .sampled_hashes
        .iter()
        .find(|sample| sample.frame_index == signal_frame)
        .map(|sample| normalize_hash(&sample.hash));

    let Some(observed_hash) = observed else {
        return Err(format!(
            "Missing title-signal hash sample at frame {} (configure hash window/stride to include this frame).",
            signal_frame
        ));
    };

    if observed_hash == expected_hash {
        Ok(true)
    } else {
        Err(format!(
            "Title signal mismatch at frame {}: expected {}, observed {}.",
            signal_frame, expected_hash, observed_hash
        ))
    }
}

fn write_outputs(config: &CliConfig, presenter: &SmokePresenter) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(&config.output_dir)?;

    let commit_sha = git_commit_sha()?;
    let title_id_value = config.title_id.as_deref().unwrap_or("unscoped-local-run");
    let mut run_fields = BTreeMap::new();
    run_fields.insert("commit_sha", quoted(&commit_sha));
    run_fields.insert("rom_id", quoted(&config.rom_id));
    run_fields.insert("runner_command", quoted(&config.runner_command));
    run_fields.insert("frame_limit", config.frame_limit.to_string());
    run_fields.insert("wall_time_limit_ms", config.wall_time_limit_ms.to_string());
    let run_json = json_object(&run_fields);
    fs::write(config.output_dir.join("run.json"), &run_json)?;

    let checkpoint_frame_index = config
        .checkpoint_start_frame
        .saturating_add(config.checkpoint_frame_count)
        .saturating_sub(1);

    let expected_hash_samples = expected_hash_sample_count(config);
    let actual_hash_samples = presenter.sampled_hashes.len() as u64;
    let has_full_hash_coverage = actual_hash_samples == expected_hash_samples;
    let title_signal_check = title_signal_matches(config, presenter);
    let title_signal_ok = title_signal_check.as_ref().is_ok_and(|matched| *matched);

    let status = if presenter.checkpoint_reached()
        && !presenter.timed_out
        && has_full_hash_coverage
        && title_signal_ok
    {
        "PASS"
    } else {
        "FAIL"
    };
    let pass_fail_reason = if let Err(reason) = title_signal_check {
        reason
    } else if !has_full_hash_coverage {
        format!(
            "Incomplete hash-window coverage: expected {} samples for start={} frame_count={} stride={}, captured {}.",
            expected_hash_samples,
            config.hash_start_frame,
            config.hash_frame_count,
            config.hash_sample_stride,
            actual_hash_samples
        )
    } else if status == "PASS" {
        format!(
            "Captured configured checkpoint window [{}..={}] within deterministic frame/time budget.",
            config.checkpoint_start_frame, checkpoint_frame_index
        )
    } else if presenter.timed_out && presenter.checkpoint_reached() {
        format!(
            "Timed out at {}ms after {} presented frames after reaching checkpoint window [{}..={}], before completing full smoke evidence requirements.",
            presenter.elapsed_ms(),
            presenter.frames_presented,
            config.checkpoint_start_frame,
            checkpoint_frame_index
        )
    } else if presenter.timed_out {
        format!(
            "Timed out at {}ms after {} presented frames before reaching checkpoint window [{}..={}].",
            presenter.elapsed_ms(),
            presenter.frames_presented,
            config.checkpoint_start_frame,
            checkpoint_frame_index
        )
    } else {
        format!(
            "Frame budget exhausted after {} frames before reaching checkpoint window [{}..={}].",
            presenter.frames_presented, config.checkpoint_start_frame, checkpoint_frame_index
        )
    };

    let mut summary_fields = BTreeMap::new();
    summary_fields.insert("status", quoted(status));
    summary_fields.insert("checkpoint_frame_index", checkpoint_frame_index.to_string());
    summary_fields.insert("pass_fail_reason", quoted(&pass_fail_reason));
    let summary_json = json_object(&summary_fields);
    fs::write(config.output_dir.join("summary.json"), &summary_json)?;

    let hashes = if presenter.sampled_hashes.is_empty() {
        if presenter.frames_presented == 0 {
            return Err(
                "no frames were presented; cannot emit schema-compatible hash_window.json".into(),
            );
        }

        vec![format!(
            "    {{\"frame_index\": {}, \"hash\": {}}}",
            config.hash_start_frame,
            quoted("missing-hash-window-sample")
        )]
    } else {
        presenter
            .sampled_hashes
            .iter()
            .map(|sample| {
                format!(
                    "    {{\"frame_index\": {}, \"hash\": {}}}",
                    sample.frame_index,
                    quoted(&sample.hash)
                )
            })
            .collect::<Vec<_>>()
    };

    let hashes_json = if hashes.is_empty() {
        "[]".to_owned()
    } else {
        format!("[\n{}\n  ]", hashes.join(",\n"))
    };

    let hash_window_json = format!(
        "{{\n  \"algorithm\": \"fnv1a64-rgb32le\",\n  \"start_frame\": {},\n  \"frame_count\": {},\n  \"sample_stride\": {},\n  \"hashes\": {}\n}}",
        config.hash_start_frame, config.hash_frame_count, config.hash_sample_stride, hashes_json
    );
    fs::write(
        config.output_dir.join("hash_window.json"),
        &hash_window_json,
    )?;

    let pass_window_json = format!(
        "{{\n  \"start_frame\": {},\n  \"frame_count\": {}\n}}",
        config.checkpoint_start_frame, config.checkpoint_frame_count
    );
    fs::write(
        config.output_dir.join("pass_window.json"),
        &pass_window_json,
    )?;

    let title_evidence_json = format!(
        "{{\n  \"run.json\": {},\n  \"summary.json\": {},\n  \"hash_window\": {},\n  \"pass_window\": {},\n  \"copyrighted_assets_committed\": false\n}}",
        run_json, summary_json, hash_window_json, pass_window_json,
    );
    fs::write(
        config.output_dir.join("title-evidence.json"),
        title_evidence_json,
    )?;

    let runner_log = format!(
        "status={status}\nrom={}\nrom_id={}\ntitle_id={}\nframes_presented={}\nelapsed_ms={}\ncheckpoint_window={}..={}\nhash_samples={}\nexpected_hash_samples={}\ntitle_signal_frame={:?}\ntitle_signal_hash={:?}\n",
        config.rom_path.display(),
        config.rom_id,
        title_id_value,
        presenter.frames_presented,
        presenter.elapsed_ms(),
        config.checkpoint_start_frame,
        checkpoint_frame_index,
        presenter.sampled_hashes.len(),
        expected_hash_samples,
        config.title_signal_frame,
        config.title_signal_hash
    );
    fs::write(config.output_dir.join("runner.log"), runner_log)?;

    println!(
        "Milestone 4 smoke harness complete: status={status}, output_dir={}",
        config.output_dir.display()
    );

    Ok(())
}

fn run(config: &CliConfig) -> Result<SmokePresenter, Box<dyn Error>> {
    let rom_bytes = fs::read(&config.rom_path).map_err(|error| {
        format!(
            "failed to read ROM '{}': {error}",
            config.rom_path.as_path().display()
        )
    })?;
    let cartridge = Cartridge::from_rom(rom_bytes).map_err(|error| {
        format!(
            "failed to parse cartridge from ROM '{}': {error:?}",
            config.rom_path.as_path().display()
        )
    })?;

    let mut emulator = Emulator::from_cartridge(cartridge);
    let mut presenter = SmokePresenter::new(config);

    run_emulation_loop(
        &mut emulator,
        &mut presenter,
        config.cycle_step,
        Some(config.frame_limit),
        None,
    )
    .map_err(|error| format!("emulation loop aborted: {error}"))?;

    if presenter.elapsed_ms() > u128::from(config.wall_time_limit_ms) {
        presenter.timed_out = true;
    }

    Ok(presenter)
}

fn main() -> process::ExitCode {
    let config = match parse_args() {
        Ok(config) => config,
        Err(error) => {
            if error.0.starts_with("milestone4_smoke usage:") {
                println!("{error}");
                return process::ExitCode::SUCCESS;
            }
            eprintln!("{error}");
            return process::ExitCode::FAILURE;
        }
    };

    let presenter = match run(&config) {
        Ok(presenter) => presenter,
        Err(error) => {
            eprintln!("error: {error}");
            return process::ExitCode::FAILURE;
        }
    };

    if let Err(error) = write_outputs(&config, &presenter) {
        eprintln!("error: failed to emit smoke outputs: {error}");
        return process::ExitCode::FAILURE;
    }

    let has_full_hash_coverage =
        presenter.sampled_hashes.len() as u64 == expected_hash_sample_count(&config);
    let title_signal_ok = title_signal_matches(&config, &presenter).is_ok_and(|matched| matched);
    if presenter.checkpoint_reached()
        && !presenter.timed_out
        && has_full_hash_coverage
        && title_signal_ok
    {
        process::ExitCode::SUCCESS
    } else {
        process::ExitCode::FAILURE
    }
}
