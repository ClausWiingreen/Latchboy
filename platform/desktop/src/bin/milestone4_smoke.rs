use std::env;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::{self, Command};
use std::time::Instant;

use clap::{Parser, ValueEnum};
use latchboy_core::{cartridge::Cartridge, Emulator};
use latchboy_desktop::{run_emulation_loop, FramePresenter};
use serde::Serialize;
use tracing::{debug, info, info_span};
use tracing_subscriber::{fmt, EnvFilter};

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

#[derive(Clone, Copy, Debug, ValueEnum)]
enum MatrixPresetId {
    #[value(name = "tetris-world")]
    Tetris,
    #[value(name = "super-mario-land-world")]
    SuperMarioLand,
    #[value(name = "legend-of-zelda-links-awakening-world")]
    LegendOfZeldaLinksAwakening,
}

impl MatrixPresetId {
    fn as_str(self) -> &'static str {
        match self {
            Self::Tetris => "tetris-world",
            Self::SuperMarioLand => "super-mario-land-world",
            Self::LegendOfZeldaLinksAwakening => "legend-of-zelda-links-awakening-world",
        }
    }

    fn preset(self) -> MatrixPreset {
        MATRIX_PRESETS
            .iter()
            .find(|preset| preset.title_id == self.as_str())
            .copied()
            .expect("matrix preset id must map to MATRIX_PRESETS")
    }
}

#[derive(Debug, Parser)]
#[command(name = "milestone4_smoke")]
struct SmokeCliArgs {
    #[arg(long)]
    rom: PathBuf,
    #[arg(long)]
    rom_id: Option<String>,
    #[arg(long, value_enum)]
    title_id: Option<MatrixPresetId>,
    #[arg(long)]
    output_dir: PathBuf,
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
    frame_limit: Option<u64>,
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
    wall_time_limit_ms: Option<u64>,
    #[arg(long, value_parser = clap::value_parser!(u64))]
    checkpoint_start_frame: Option<u64>,
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
    checkpoint_frame_count: Option<u64>,
    #[arg(long, value_parser = clap::value_parser!(u64))]
    title_signal_frame: Option<u64>,
    #[arg(long, value_parser = parse_hash_arg)]
    title_signal_hash: Option<String>,
    #[arg(long, value_parser = clap::value_parser!(u64))]
    hash_start_frame: Option<u64>,
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
    hash_frame_count: Option<u64>,
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
    hash_sample_stride: Option<u64>,
    #[arg(
        long,
        value_parser = clap::value_parser!(u32).range(1..),
        default_value_t = DEFAULT_CYCLE_STEP
    )]
    cycle_step: u32,
}

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

#[derive(Debug, Serialize)]
struct RunJson {
    commit_sha: String,
    rom_id: String,
    runner_command: String,
    frame_limit: u64,
    wall_time_limit_ms: u64,
}

#[derive(Debug, Serialize)]
struct SummaryJson {
    status: String,
    checkpoint_frame_index: u64,
    pass_fail_reason: String,
}

#[derive(Debug, Serialize)]
struct HashWindowHashEntry {
    frame_index: u64,
    hash: String,
}

#[derive(Debug, Serialize)]
struct HashWindowJson {
    algorithm: String,
    start_frame: u64,
    frame_count: u64,
    sample_stride: u64,
    hashes: Vec<HashWindowHashEntry>,
}

#[derive(Debug, Serialize)]
struct PassWindowJson {
    start_frame: u64,
    frame_count: u64,
}

#[derive(Debug, Serialize)]
struct TitleEvidenceJson {
    #[serde(rename = "run.json")]
    run_json: RunJson,
    #[serde(rename = "summary.json")]
    summary_json: SummaryJson,
    hash_window: HashWindowJson,
    pass_window: PassWindowJson,
    copyrighted_assets_committed: bool,
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
    logged_frame_budget_exhausted: bool,
    logged_time_budget_exhausted: bool,
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
            logged_frame_budget_exhausted: false,
            logged_time_budget_exhausted: false,
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
        self.frames_presented < self.frame_limit
            && self.elapsed_ms() <= u128::from(self.wall_time_limit_ms)
    }

    fn poll_events(&mut self) -> Result<(), Self::Error> {
        Ok(())
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
            if !self.logged_time_budget_exhausted {
                self.logged_time_budget_exhausted = true;
                debug!(
                    elapsed_ms = self.elapsed_ms(),
                    wall_time_limit_ms = self.wall_time_limit_ms,
                    "time budget exhaustion"
                );
            }
        }
        if self.frames_presented >= self.frame_limit && !self.logged_frame_budget_exhausted {
            self.logged_frame_budget_exhausted = true;
            debug!(
                frames_presented = self.frames_presented,
                frame_limit = self.frame_limit,
                "frame budget exhaustion"
            );
        }
        Ok(())
    }
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = fmt()
        .with_env_filter(env_filter)
        .compact()
        .with_target(false)
        .try_init();
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

fn normalize_hash(value: &str) -> String {
    let trimmed = value.trim();
    let without_prefix = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    without_prefix.to_ascii_lowercase()
}

fn parse_hash_u64(value: &str) -> Result<u64, String> {
    let normalized = normalize_hash(value);
    u64::from_str_radix(&normalized, 16)
        .map_err(|_| format!("invalid hash '{}' (expected 1-16 hex digits)", value.trim()))
}

fn parse_hash_arg(value: &str) -> Result<String, String> {
    let normalized = normalize_hash(value);
    parse_hash_u64(value)?;
    Ok(normalized)
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

fn parse_args() -> Result<CliConfig, String> {
    let provided_args = env::args().skip(1).collect::<Vec<_>>();
    let args = SmokeCliArgs::parse();
    let title_id = args.title_id.map(|id| id.as_str().to_owned());
    let selected_preset = args.title_id.map(|id| id.preset());

    let rom_id = args.rom_id.unwrap_or_else(|| {
        args.rom
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("unknown-rom")
            .to_owned()
    });

    let frame_limit = args
        .frame_limit
        .or(selected_preset.map(|preset| preset.frame_limit))
        .unwrap_or(300);
    let wall_time_limit_ms = args
        .wall_time_limit_ms
        .or(selected_preset.map(|preset| preset.wall_time_limit_ms))
        .unwrap_or(10_000);
    let checkpoint_start_frame = args
        .checkpoint_start_frame
        .or(selected_preset.map(|preset| preset.checkpoint_start_frame))
        .unwrap_or(frame_limit.saturating_sub(120));
    let checkpoint_frame_count = args
        .checkpoint_frame_count
        .or(selected_preset.map(|preset| preset.checkpoint_frame_count))
        .unwrap_or(120);
    let checkpoint_frame_index = checkpoint_start_frame
        .saturating_add(checkpoint_frame_count)
        .saturating_sub(1);

    let hash_start_frame = args.hash_start_frame.unwrap_or(checkpoint_start_frame);
    let hash_frame_count = args.hash_frame_count.unwrap_or(checkpoint_frame_count);
    let hash_sample_stride = args.hash_sample_stride.unwrap_or(1);
    let title_signal_frame = args.title_signal_frame.or_else(|| {
        title_id.as_ref().map(|_| {
            default_title_signal_frame(
                checkpoint_frame_index,
                hash_start_frame,
                hash_frame_count,
                hash_sample_stride,
            )
        })
    });
    let cycle_step = args.cycle_step;
    let runner_command = format!(
        "cargo run -p latchboy-desktop --bin milestone4_smoke -- {}",
        provided_args
            .iter()
            .map(|arg| shell_escape_arg(arg))
            .collect::<Vec<_>>()
            .join(" ")
    );

    if frame_limit == 0 {
        return Err("--frame-limit must be greater than zero".to_owned());
    }
    if wall_time_limit_ms == 0 {
        return Err("--wall-time-limit-ms must be greater than zero".to_owned());
    }
    if checkpoint_frame_count == 0 {
        return Err("--checkpoint-frame-count must be greater than zero".to_owned());
    }
    if hash_frame_count == 0 {
        return Err("--hash-frame-count must be greater than zero".to_owned());
    }
    if hash_sample_stride == 0 {
        return Err("--hash-sample-stride must be greater than zero".to_owned());
    }
    if cycle_step == 0 {
        return Err("--cycle-step must be greater than zero".to_owned());
    }
    if title_id.is_some() && args.title_signal_hash.is_none() {
        return Err(
            "--title-id requires --title-signal-hash so PASS can be gated on title-specific signal evidence".to_owned(),
        );
    }
    if args.title_signal_hash.is_some() && title_signal_frame.is_none() && title_id.is_none() {
        return Err(
            "--title-signal-hash requires --title-signal-frame when --title-id is not provided"
                .to_owned(),
        );
    }
    if let Some(frame) = title_signal_frame {
        if !frame_is_hash_sample(
            frame,
            hash_start_frame,
            hash_frame_count,
            hash_sample_stride,
        ) {
            return Err(format!(
                "--title-signal-frame {} is not sampled by hash window start={} frame_count={} stride={}",
                frame, hash_start_frame, hash_frame_count, hash_sample_stride
            ));
        }
    }

    Ok(CliConfig {
        rom_path: args.rom,
        rom_id,
        title_id,
        output_dir: args.output_dir,
        runner_command,
        frame_limit,
        wall_time_limit_ms,
        checkpoint_start_frame,
        checkpoint_frame_count,
        title_signal_frame,
        title_signal_hash: args.title_signal_hash,
        hash_start_frame,
        hash_frame_count,
        hash_sample_stride,
        cycle_step,
    })
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
    let signal_frame = config.title_signal_frame.ok_or_else(|| {
        "Missing --title-signal-frame for --title-signal-hash (or provide --title-id to use preset-derived defaults)."
            .to_owned()
    })?;

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

    let expected_hash_value = parse_hash_u64(expected_hash)
        .map_err(|reason| format!("Invalid --title-signal-hash: {}.", reason))?;
    let observed_hash_value = parse_hash_u64(&observed_hash)
        .map_err(|reason| format!("Captured non-hex title signal hash: {}.", reason))?;

    if observed_hash_value == expected_hash_value {
        Ok(true)
    } else {
        Err(format!(
            "Title signal mismatch at frame {}: expected 0x{:016x}, observed 0x{:016x}.",
            signal_frame, expected_hash_value, observed_hash_value
        ))
    }
}

fn write_outputs(config: &CliConfig, presenter: &SmokePresenter) -> Result<(), Box<dyn Error>> {
    let _artifact_span =
        info_span!("artifact_write", output_dir = %config.output_dir.display()).entered();
    fs::create_dir_all(&config.output_dir)?;

    let commit_sha = git_commit_sha()?;
    let title_id_value = config.title_id.as_deref().unwrap_or("unscoped-local-run");
    let run_json = RunJson {
        commit_sha,
        rom_id: config.rom_id.clone(),
        runner_command: config.runner_command.clone(),
        frame_limit: config.frame_limit,
        wall_time_limit_ms: config.wall_time_limit_ms,
    };
    fs::write(
        config.output_dir.join("run.json"),
        serde_json::to_string_pretty(&run_json)?,
    )?;
    debug!(path = %config.output_dir.join("run.json").display(), "artifact written");

    let checkpoint_frame_index = config
        .checkpoint_start_frame
        .saturating_add(config.checkpoint_frame_count)
        .saturating_sub(1);

    let expected_hash_samples = expected_hash_sample_count(config);
    let actual_hash_samples = presenter.sampled_hashes.len() as u64;
    let has_full_hash_coverage = actual_hash_samples == expected_hash_samples;
    let no_frames_presented = presenter.frames_presented == 0;
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
    } else if no_frames_presented {
        "No frames were presented before the run terminated; emitted placeholder hash evidence for schema compatibility."
            .to_owned()
    } else if !presenter.checkpoint_reached() {
        format!(
            "Frame budget exhausted after {} frames before reaching checkpoint window [{}..={}].",
            presenter.frames_presented, config.checkpoint_start_frame, checkpoint_frame_index
        )
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
    } else {
        format!(
            "Failed smoke checks despite reaching checkpoint window [{}..={}].",
            config.checkpoint_start_frame, checkpoint_frame_index
        )
    };

    let summary_json = SummaryJson {
        status: status.to_owned(),
        checkpoint_frame_index,
        pass_fail_reason,
    };
    fs::write(
        config.output_dir.join("summary.json"),
        serde_json::to_string_pretty(&summary_json)?,
    )?;
    debug!(path = %config.output_dir.join("summary.json").display(), "artifact written");

    let hashes = if presenter.sampled_hashes.is_empty() {
        let placeholder_hash = if no_frames_presented {
            "missing-hash-window-sample-no-frames"
        } else {
            "missing-hash-window-sample"
        };
        vec![HashWindowHashEntry {
            frame_index: config.hash_start_frame,
            hash: placeholder_hash.to_owned(),
        }]
    } else {
        presenter
            .sampled_hashes
            .iter()
            .map(|sample| HashWindowHashEntry {
                frame_index: sample.frame_index,
                hash: sample.hash.clone(),
            })
            .collect::<Vec<_>>()
    };

    let hash_window_json = HashWindowJson {
        algorithm: "fnv1a64-rgb32le".to_owned(),
        start_frame: config.hash_start_frame,
        frame_count: config.hash_frame_count,
        sample_stride: config.hash_sample_stride,
        hashes,
    };
    fs::write(
        config.output_dir.join("hash_window.json"),
        serde_json::to_string_pretty(&hash_window_json)?,
    )?;
    debug!(path = %config.output_dir.join("hash_window.json").display(), "artifact written");

    let pass_window_json = PassWindowJson {
        start_frame: config.checkpoint_start_frame,
        frame_count: config.checkpoint_frame_count,
    };
    fs::write(
        config.output_dir.join("pass_window.json"),
        serde_json::to_string_pretty(&pass_window_json)?,
    )?;
    debug!(path = %config.output_dir.join("pass_window.json").display(), "artifact written");

    let title_evidence_json = TitleEvidenceJson {
        run_json,
        summary_json,
        hash_window: hash_window_json,
        pass_window: pass_window_json,
        copyrighted_assets_committed: false,
    };
    fs::write(
        config.output_dir.join("title-evidence.json"),
        serde_json::to_string_pretty(&title_evidence_json)?,
    )?;
    debug!(path = %config.output_dir.join("title-evidence.json").display(), "artifact written");

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
    debug!(path = %config.output_dir.join("runner.log").display(), "artifact written");

    println!(
        "Milestone 4 smoke harness complete: status={status}, output_dir={}",
        config.output_dir.display()
    );

    Ok(())
}

fn run(config: &CliConfig) -> Result<SmokePresenter, Box<dyn Error>> {
    let _run_span = info_span!("smoke_run", rom = %config.rom_path.display()).entered();
    info!("smoke run starting");
    let _rom_span = info_span!("rom_load").entered();
    let rom_bytes = fs::read(&config.rom_path).map_err(|error| {
        format!(
            "failed to read ROM '{}': {error}",
            config.rom_path.as_path().display()
        )
    })?;
    info!(rom_size = rom_bytes.len(), "rom loaded");
    drop(_rom_span);
    let _cart_span = info_span!("cartridge_parse").entered();
    let cartridge = Cartridge::from_rom(rom_bytes).map_err(|error| {
        format!(
            "failed to parse cartridge from ROM '{}': {error:?}",
            config.rom_path.as_path().display()
        )
    })?;
    info!("cartridge parsed");
    drop(_cart_span);

    let mut emulator = Emulator::from_cartridge(cartridge);
    let mut presenter = SmokePresenter::new(config);

    let _frame_loop_span = info_span!("frame_loop").entered();
    info!("frame loop starting");
    run_emulation_loop(
        &mut emulator,
        &mut presenter,
        config.cycle_step,
        Some(config.frame_limit),
        None,
    )
    .map_err(|error| format!("emulation loop aborted: {error}"))?;
    info!(
        frames_presented = presenter.frames_presented,
        "frame loop ended"
    );

    if presenter.elapsed_ms() > u128::from(config.wall_time_limit_ms) {
        presenter.timed_out = true;
    }

    Ok(presenter)
}

fn main() -> process::ExitCode {
    init_tracing();
    let config = match parse_args() {
        Ok(config) => config,
        Err(error) => {
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
