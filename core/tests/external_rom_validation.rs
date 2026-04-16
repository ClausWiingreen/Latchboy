use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Instant;

use latchboy_core::{
    cartridge::Cartridge,
    observability::{EmulatorEvent, TraceBuffer},
    Emulator,
};

const CYCLES_PER_FRAME: u32 = 70_224;
const ROM_MANIFEST_PATH: &str = "../tests/rom_manifest.toml";
const ROM_ROOT_ENV: &str = "LATCHBOY_ROM_ROOT";
const TRACE_EVENTS_ON_FAILURE: usize = 64;

#[derive(Debug)]
struct RomManifest {
    roms: Vec<RomEntry>,
}

#[derive(Debug, Default)]
struct RomEntry {
    id: String,
    suite: String,
    path: String,
    milestone: u8,
    required: bool,
    cycle_limit: u64,
    frame_limit: u64,
    wall_time_limit_ms: u64,
    pass_condition: PassCondition,
}

#[derive(Debug, Default, Clone, Copy)]
enum PassCondition {
    #[default]
    None,
    BlarggMem,
    BlarggRegisters,
    MooneyeRegisters,
}

#[derive(Debug)]
struct RomRunResult {
    elapsed_ms: u128,
    executed_cycles: u64,
    final_hash: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PassCheck {
    Pending,
    Passed,
    Failed,
}

fn parse_manifest(manifest_path: &Path) -> RomManifest {
    let manifest_contents = fs::read_to_string(manifest_path).unwrap_or_else(|error| {
        panic!(
            "failed to read ROM manifest at {}: {error:?}",
            manifest_path.display()
        )
    });

    let mut roms = Vec::new();
    let mut current: Option<RomEntry> = None;

    for (line_number, raw_line) in manifest_contents.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line == "[[roms]]" {
            if let Some(entry) = current.take() {
                roms.push(entry);
            }
            current = Some(RomEntry::default());
            continue;
        }

        let (key, value) = line.split_once('=').unwrap_or_else(|| {
            panic!(
                "invalid manifest syntax at {}:{}: expected key = value",
                manifest_path.display(),
                line_number + 1
            )
        });

        let entry = current.as_mut().unwrap_or_else(|| {
            panic!(
                "manifest entry fields must be under [[roms]] heading at {}:{}",
                manifest_path.display(),
                line_number + 1
            )
        });

        let key = key.trim();
        let value = strip_inline_comment(value.trim());
        match key {
            "id" => entry.id = parse_string(value),
            "suite" => entry.suite = parse_string(value),
            "path" => entry.path = parse_string(value),
            "milestone" => entry.milestone = parse_u64(value) as u8,
            "required" => entry.required = parse_bool(value),
            "cycle_limit" => entry.cycle_limit = parse_u64(value),
            "frame_limit" => entry.frame_limit = parse_u64(value),
            "wall_time_limit_ms" => entry.wall_time_limit_ms = parse_u64(value),
            "pass_condition" => entry.pass_condition = parse_pass_condition(value),
            _ => {
                panic!(
                    "unknown key '{key}' at {}:{}",
                    manifest_path.display(),
                    line_number + 1
                );
            }
        }
    }

    if let Some(entry) = current.take() {
        roms.push(entry);
    }

    assert!(
        !roms.is_empty(),
        "ROM manifest at {} must define at least one [[roms]] entry",
        manifest_path.display()
    );

    RomManifest { roms }
}

fn parse_string(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or_else(|| panic!("expected quoted string, got '{value}'"))
        .to_owned()
}

fn strip_inline_comment(value: &str) -> &str {
    let mut in_string = false;
    let mut escape_active = false;

    for (index, ch) in value.char_indices() {
        if ch == '"' && !escape_active {
            in_string = !in_string;
        }

        if ch == '#' && !in_string {
            return value[..index].trim_end();
        }

        escape_active = ch == '\\' && !escape_active;
        if ch != '\\' {
            escape_active = false;
        }
    }

    value.trim_end()
}

fn parse_u64(value: &str) -> u64 {
    value
        .replace('_', "")
        .parse::<u64>()
        .unwrap_or_else(|_| panic!("expected positive integer, got '{value}'"))
}

fn parse_bool(value: &str) -> bool {
    match value {
        "true" => true,
        "false" => false,
        _ => panic!("expected boolean true/false, got '{value}'"),
    }
}

fn parse_pass_condition(value: &str) -> PassCondition {
    match parse_string(value).as_str() {
        "none" => PassCondition::None,
        "blargg_mem" => PassCondition::BlarggMem,
        "blargg_registers" => PassCondition::BlarggRegisters,
        "mooneye_registers" => PassCondition::MooneyeRegisters,
        other => panic!(
            "unknown pass_condition '{other}', expected one of: none, blargg_mem, blargg_registers, mooneye_registers"
        ),
    }
}

fn rom_root_from_env() -> Option<PathBuf> {
    let value = std::env::var(ROM_ROOT_ENV).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    Some(PathBuf::from(trimmed))
}

fn is_noop_pass_condition(pass_condition: PassCondition) -> bool {
    matches!(pass_condition, PassCondition::None)
}

fn check_pass_condition(emulator: &Emulator, rom: &RomEntry) -> PassCheck {
    match rom.pass_condition {
        PassCondition::None => PassCheck::Passed,
        PassCondition::BlarggMem => {
            let signature = [
                emulator.bus().read8(0xA001),
                emulator.bus().read8(0xA002),
                emulator.bus().read8(0xA003),
            ];

            if signature != [0xDE, 0xB0, 0x61] {
                return PassCheck::Pending;
            }

            let status = emulator.bus().read8(0xA000);
            match status {
                0x00 => PassCheck::Passed,
                0x80 => PassCheck::Pending,
                _ => PassCheck::Failed,
            }
        }
        PassCondition::BlarggRegisters => {
            let signature = [
                emulator.bus().read8(emulator.cpu().pc()),
                emulator.bus().read8(emulator.cpu().pc().wrapping_add(1)),
            ];

            if signature != [0x18, 0xFE] {
                return PassCheck::Pending;
            }

            match emulator.cpu().registers().a {
                0x00 => PassCheck::Passed,
                _ => PassCheck::Failed,
            }
        }
        PassCondition::MooneyeRegisters => {
            let registers = emulator.cpu().registers();
            if registers.b == 3
                && registers.c == 5
                && registers.d == 8
                && registers.e == 13
                && registers.h == 21
                && registers.l == 34
            {
                PassCheck::Passed
            } else {
                PassCheck::Pending
            }
        }
    }
}

fn run_rom(rom_root: &Path, rom: &RomEntry) -> Result<RomRunResult, String> {
    let rom_path = rom_root.join(&rom.path);
    let rom_bytes = fs::read(&rom_path)
        .map_err(|error| format!("failed to read ROM {}: {error:?}", rom_path.display()))?;
    let cartridge = Cartridge::from_rom(rom_bytes).map_err(|error| {
        format!(
            "failed to parse ROM cartridge for {} ({}): {error:?}",
            rom.id,
            rom_path.display()
        )
    })?;

    let frame_cycles = rom.frame_limit.saturating_mul(CYCLES_PER_FRAME as u64);
    let cycle_budget = rom.cycle_limit.min(frame_cycles);

    let mut emulator = Emulator::from_cartridge(cartridge.clone());
    let start = Instant::now();

    let mut executed_cycles = 0u64;
    while executed_cycles < cycle_budget {
        let step = (cycle_budget - executed_cycles).min(4_096) as u32;
        emulator.step_cycles(step);
        executed_cycles += u64::from(step);

        if let Some(opcode) = emulator.cpu().last_unimplemented_opcode() {
            let trace =
                collect_recent_trace(cartridge.clone(), executed_cycles, TRACE_EVENTS_ON_FAILURE);
            return Err(format!(
                "encountered unimplemented opcode 0x{opcode:02X} after {executed_cycles} cycles\nrecent execution trace:\n{}",
                format_trace(&trace)
            ));
        }

        let elapsed_ms = start.elapsed().as_millis();
        if elapsed_ms > u128::from(rom.wall_time_limit_ms) {
            return Err(format!(
                "exceeded wall-time budget of {}ms at {}ms\nstate at timeout: {}",
                rom.wall_time_limit_ms,
                elapsed_ms,
                format_timeout_state(&emulator, executed_cycles)
            ));
        }

        match check_pass_condition(&emulator, rom) {
            PassCheck::Passed => {
                let mut hasher = DefaultHasher::new();
                emulator.hash(&mut hasher);

                return Ok(RomRunResult {
                    elapsed_ms,
                    executed_cycles,
                    final_hash: hasher.finish(),
                });
            }
            PassCheck::Failed => {
                let trace = collect_recent_trace(
                    cartridge.clone(),
                    executed_cycles,
                    TRACE_EVENTS_ON_FAILURE,
                );
                return Err(format!(
                    "ROM reported failure via pass_condition {:?} after {executed_cycles} cycles\nrecent execution trace:\n{}",
                    rom.pass_condition,
                    format_trace(&trace)
                ));
            }
            PassCheck::Pending => {}
        }
    }

    let trace = collect_recent_trace(cartridge, executed_cycles, TRACE_EVENTS_ON_FAILURE);
    Err(format!(
        "ROM did not satisfy pass_condition {:?} within {} cycles (frame_limit {}, wall_time_limit_ms {})\nrecent execution trace:\n{}",
        rom.pass_condition,
        rom.cycle_limit,
        rom.frame_limit,
        rom.wall_time_limit_ms,
        format_trace(&trace)
    ))
}

fn collect_recent_trace(cartridge: Cartridge, cycles: u64, capacity: usize) -> TraceBuffer {
    let mut emulator = Emulator::from_cartridge(cartridge);
    let mut trace = TraceBuffer::new(capacity);
    let mut executed_cycles = 0u64;

    while executed_cycles < cycles {
        let step = (cycles - executed_cycles).min(4_096) as u32;
        emulator.step_cycles_with_observer(step, &mut trace);
        executed_cycles += u64::from(step);
    }

    trace
}

fn format_trace(trace: &TraceBuffer) -> String {
    if trace.is_empty() {
        return "<empty>".to_owned();
    }

    trace
        .iter()
        .enumerate()
        .map(|(index, event)| match event {
            EmulatorEvent::CpuStep(step) => format!(
                "#{index:02} cycle={}..{} opcode={} pc={:04X}->{:04X} sp={:04X}->{:04X} a={:02X} f={:02X} bc={:04X} de={:04X} hl={:04X} ime={} halted={}",
                step.start_cycle,
                step.end_cycle,
                step
                    .opcode_hint
                    .map(|opcode| format!("0x{opcode:02X}"))
                    .unwrap_or_else(|| "none".to_owned()),
                step.pc_before,
                step.pc_after,
                step.sp_before,
                step.sp_after,
                step.registers_after.a,
                step.registers_after.f,
                step.registers_after.bc(),
                step.registers_after.de(),
                step.registers_after.hl(),
                step.ime_after,
                step.halted_after
            ),
            EmulatorEvent::HaltedFastForward(halted) => format!(
                "#{index:02} cycle={}..{} HALT_FAST_FORWARD pc={:04X} cycles={} if={:02X} ie={:02X}",
                halted.start_cycle,
                halted.end_cycle,
                halted.pc,
                halted.cycles,
                halted.interrupt_flag,
                halted.interrupt_enable
            ),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_timeout_state(emulator: &Emulator, executed_cycles: u64) -> String {
    let registers = emulator.cpu().registers();
    format!(
        "cycle={} pc={:04X} sp={:04X} a={:02X} f={:02X} bc={:04X} de={:04X} hl={:04X} ime={} halted={} if={:02X} ie={:02X}",
        executed_cycles,
        emulator.cpu().pc(),
        emulator.cpu().sp(),
        registers.a,
        registers.f,
        registers.bc(),
        registers.de(),
        registers.hl(),
        emulator.cpu().ime(),
        emulator.cpu().halted(),
        emulator.bus().read8(0xFF0F),
        emulator.bus().read8(0xFFFF)
    )
}

#[test]
fn rom_manifest_registers_required_milestone_2_suites() {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(ROM_MANIFEST_PATH);
    let manifest = parse_manifest(&manifest_path);

    assert!(
        manifest
            .roms
            .iter()
            .any(|rom| rom.required && rom.milestone == 2 && rom.suite == "blargg_cpu_instrs"),
        "manifest must include at least one required milestone 2 Blargg cpu_instrs ROM"
    );
    assert!(
        manifest
            .roms
            .iter()
            .any(|rom| rom.required && rom.milestone == 2 && rom.suite == "blargg_instr_timing"),
        "manifest must include at least one required milestone 2 Blargg instr_timing ROM"
    );
    assert!(
        manifest
            .roms
            .iter()
            .any(|rom| !rom.required && rom.suite == "mooneye_acceptance_cpu"),
        "manifest must include at least one deferred Mooneye CPU acceptance ROM entry"
    );

    for rom in &manifest.roms {
        assert!(
            rom.cycle_limit > 0,
            "{} cycle_limit must be positive",
            rom.id
        );
        assert!(
            rom.frame_limit > 0,
            "{} frame_limit must be positive",
            rom.id
        );
        assert!(
            rom.wall_time_limit_ms > 0,
            "{} wall_time_limit_ms must be positive",
            rom.id
        );

        if rom.required && rom.milestone == 2 {
            assert!(
                !is_noop_pass_condition(rom.pass_condition),
                "{} is required for milestone 2 and must not use pass_condition = \"none\"",
                rom.id
            );
        }
    }
}

#[test]
fn manifest_parser_accepts_inline_toml_comments() {
    let temp_dir = std::env::temp_dir();
    let manifest_path = temp_dir.join(format!(
        "latchboy-inline-comment-manifest-{}.toml",
        std::process::id()
    ));

    let content = r#"
[[roms]]
id = "inline-comment-case" # identifier
suite = "blargg_cpu_instrs" # suite
path = "blargg/cpu_instrs/individual/01-special.gb" # path
milestone = 2 # backlog milestone
required = true # should be parsed as bool
cycle_limit = 20_000_000 # numeric with underscore
frame_limit = 300 # frame budget
wall_time_limit_ms = 8_000 # time budget
pass_condition = "blargg_mem" # suite signal
"#;

    fs::write(&manifest_path, content).expect("temporary manifest should be writable");
    let manifest = parse_manifest(&manifest_path);
    fs::remove_file(&manifest_path).expect("temporary manifest should be removable");

    assert_eq!(manifest.roms.len(), 1);
    let rom = &manifest.roms[0];
    assert_eq!(rom.id, "inline-comment-case");
    assert!(rom.required);
    assert_eq!(rom.cycle_limit, 20_000_000);
    assert_eq!(rom.frame_limit, 300);
    assert_eq!(rom.wall_time_limit_ms, 8_000);
}

#[test]
fn required_milestone_2_roms_pass_under_external_validation_flow() {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(ROM_MANIFEST_PATH);
    let manifest = parse_manifest(&manifest_path);

    let Some(rom_root) = rom_root_from_env() else {
        eprintln!(
            "skipping required ROM run: set {ROM_ROOT_ENV} to execute external ROM validation"
        );
        return;
    };

    assert!(
        rom_root.is_dir(),
        "{ROM_ROOT_ENV} must point to a directory, got {}",
        rom_root.display()
    );

    let required_m2_roms: Vec<&RomEntry> = manifest
        .roms
        .iter()
        .filter(|rom| rom.required && rom.milestone == 2)
        .collect();

    assert!(
        !required_m2_roms.is_empty(),
        "manifest must define required milestone 2 ROM cases"
    );

    for rom in &required_m2_roms {
        assert!(
            !is_noop_pass_condition(rom.pass_condition),
            "{} is required for milestone 2 and must not use pass_condition = \"none\"",
            rom.id
        );
    }

    let mut failures = Vec::new();
    for rom in required_m2_roms {
        if let Err(error) = run_rom(&rom_root, rom) {
            failures.push(format!("{} ({}): {error:?}", rom.id, rom.path));
        }
    }

    assert!(
        failures.is_empty(),
        "required milestone 2 ROM validation failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn required_milestone_2_rom_runs_are_deterministic() {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(ROM_MANIFEST_PATH);
    let manifest = parse_manifest(&manifest_path);

    let Some(rom_root) = rom_root_from_env() else {
        eprintln!("skipping deterministic ROM check: set {ROM_ROOT_ENV}");
        return;
    };

    let required_m2_roms: Vec<&RomEntry> = manifest
        .roms
        .iter()
        .filter(|rom| rom.required && rom.milestone == 2)
        .collect();

    assert!(
        !required_m2_roms.is_empty(),
        "manifest must define required milestone 2 ROM cases"
    );

    for rom in required_m2_roms {
        let first = run_rom(&rom_root, rom).expect("first ROM run should succeed");
        let second = run_rom(&rom_root, rom).expect("second ROM run should succeed");

        assert_eq!(
            first.executed_cycles, second.executed_cycles,
            "deterministic cycle total mismatch for {}",
            rom.id
        );
        assert_eq!(
            first.final_hash, second.final_hash,
            "deterministic state hash mismatch for {}",
            rom.id
        );

        assert!(
            first.elapsed_ms <= u128::from(rom.wall_time_limit_ms),
            "first run exceeded wall-time limit for {}",
            rom.id
        );
        assert!(
            second.elapsed_ms <= u128::from(rom.wall_time_limit_ms),
            "second run exceeded wall-time limit for {}",
            rom.id
        );
    }
}
