use std::collections::VecDeque;
use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use latchboy_core::{
    cartridge::Cartridge,
    interrupts,
    observability::{
        CpuStepObservation, EmulatorEvent, EmulatorObserver, HaltedFastForwardObservation,
        WatchIoAccessType, WatchIoObservation,
    },
    Emulator,
};

const DEFAULT_CYCLE_STEP: u32 = 1;
const LOOP_WINDOW_MIN: usize = 2;
const LOOP_WINDOW_MAX: usize = 8;
const LOOP_WINDOW_PREFERRED: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StepSignature {
    opcode: Option<u8>,
    pc_before: u16,
    pc_after: u16,
    operand1: Option<u8>,
    operand2: Option<u8>,
    branch_taken: bool,
    interrupt_entry: bool,
    ppu_ly_before: u8,
    ppu_ly_after: u8,
}

impl StepSignature {
    fn from_observation(observation: &CpuStepObservation) -> Self {
        Self {
            opcode: observation.opcode_hint,
            pc_before: observation.pc_before,
            pc_after: observation.pc_after,
            operand1: observation.operand1_before,
            operand2: observation.operand2_before,
            branch_taken: observation.pc_after
                != observation
                    .pc_before
                    .wrapping_add(instruction_len(observation)),
            interrupt_entry: observation.ime_before
                && !observation.ime_after
                && observation.sp_after != observation.sp_before,
            ppu_ly_before: observation.ppu_before.ly,
            ppu_ly_after: observation.ppu_after.ly,
        }
    }
}

#[derive(Debug, Clone)]
struct PendingStep {
    step_index: u64,
    text: String,
    start_cycle: u64,
    end_cycle: u64,
    signature: StepSignature,
}

#[derive(Debug, Clone)]
struct LoopState {
    window: Vec<PendingStep>,
    repetitions: u64,
    ly_observed_min: u8,
    ly_observed_max: u8,
    ly_terminating: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopKind {
    WaitLyVblank,
}

#[derive(Debug, Clone)]
struct LoopSummary {
    kind: LoopKind,
    confidence_high: bool,
    has_compare_value: bool,
}

fn instruction_len(observation: &CpuStepObservation) -> u16 {
    match observation.opcode_hint {
        Some(
            0x3E | 0x06 | 0x0E | 0x16 | 0x1E | 0x26 | 0x2E | 0x36 | 0x18 | 0x20 | 0x28 | 0x30
            | 0x38 | 0xC6 | 0xCE | 0xD6 | 0xDE | 0xE0 | 0xE6 | 0xEE | 0xF0 | 0xF6 | 0xFE,
        ) => 2,
        Some(0x01 | 0x11 | 0x21 | 0x31 | 0x08 | 0xC3 | 0xC2 | 0xCA | 0xD2 | 0xDA | 0xCD) => 3,
        _ => 1,
    }
}

#[derive(Debug, Clone, Copy)]
enum TraceFormat {
    Minimal,
    Normal,
    Full,
}

impl TraceFormat {
    fn parse(value: &str) -> Result<Self, UsageError> {
        match value {
            "minimal" => Ok(Self::Minimal),
            "normal" => Ok(Self::Normal),
            "full" => Ok(Self::Full),
            _ => Err(UsageError(format!(
                "invalid --format value '{value}': expected one of: minimal, normal, full"
            ))),
        }
    }
}

#[derive(Debug)]
struct UsageError(String);

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for UsageError {}

#[derive(Debug)]
struct CliConfig {
    rom_path: PathBuf,
    output_path: PathBuf,
    cycle_step: u32,
    max_steps: Option<u64>,
    max_cycles: Option<u64>,
    exit_on_jr_fe: bool,
    exit_on_unimplemented: bool,
    watch_io: bool,
    format: TraceFormat,
    summarize_waits: bool,
    summarize_waits_overridden: bool,
}

enum CliParseResult {
    Help,
    Config(CliConfig),
}

#[derive(Debug)]
enum ExitReason {
    MaxStepsReached { limit: u64 },
    MaxCyclesReached { limit: u64 },
    JrFeInfiniteLoop { pc: u16 },
    UnimplementedOpcode { opcode: u8, pc: u16 },
}

struct TraceCollector<'a> {
    writer: &'a mut BufWriter<fs::File>,
    config: &'a CliConfig,
    cpu_steps: u64,
    budget_steps: u64,
    executed_cycles: u64,
    last_cpu_pc_before: Option<u16>,
    exit_reason: Option<ExitReason>,
    io_error: Option<io::Error>,
    pending_steps: VecDeque<PendingStep>,
    loop_state: Option<LoopState>,
}

impl<'a> TraceCollector<'a> {
    fn new(writer: &'a mut BufWriter<fs::File>, config: &'a CliConfig) -> Self {
        Self {
            writer,
            config,
            cpu_steps: 0,
            budget_steps: 0,
            executed_cycles: 0,
            last_cpu_pc_before: None,
            exit_reason: None,
            io_error: None,
            pending_steps: VecDeque::with_capacity(LOOP_WINDOW_MAX * 4),
            loop_state: None,
        }
    }
}

impl<'a> TraceCollector<'a> {
    fn flush_pending_raw(&mut self) -> io::Result<()> {
        while let Some(step) = self.pending_steps.pop_front() {
            writeln!(self.writer, "{}", step.text)?;
        }
        Ok(())
    }

    fn flush_loop_summary(&mut self) -> io::Result<()> {
        if let Some(state) = self.loop_state.take() {
            let loop_start_step = state
                .window
                .first()
                .map(|step| step.step_index)
                .unwrap_or(u64::MAX);
            while self
                .pending_steps
                .front()
                .is_some_and(|step| step.step_index < loop_start_step)
            {
                if let Some(step) = self.pending_steps.pop_front() {
                    writeln!(self.writer, "{}", step.text)?;
                }
            }
            let semantic = summarize_wait_loop(&state.window);
            let emit_semantic = self.config.summarize_waits
                && semantic.as_ref().is_some_and(|summary| {
                    summary.kind == LoopKind::WaitLyVblank && summary.has_compare_value
                });
            let confidence_high = semantic
                .as_ref()
                .is_some_and(|summary| summary.confidence_high);
            let force_raw_for_full = matches!(self.config.format, TraceFormat::Full)
                && !self.config.summarize_waits_overridden;
            let emit_raw_loop = force_raw_for_full || !confidence_high;
            if state.repetitions > 1 && (!emit_semantic || emit_raw_loop) {
                let start = state.window.first().map(|s| s.start_cycle).unwrap_or(0);
                let single_window_end = state.window.last().map(|s| s.end_cycle).unwrap_or(start);
                let single_window_cycles = single_window_end.saturating_sub(start);
                let end =
                    start.saturating_add(single_window_cycles.saturating_mul(state.repetitions));
                let first_pc = state
                    .window
                    .first()
                    .map(|s| s.signature.pc_before)
                    .unwrap_or(0);
                let last_pc = state
                    .window
                    .last()
                    .map(|s| s.signature.pc_after)
                    .unwrap_or(first_pc);
                let body = state
                    .window
                    .iter()
                    .map(|s| format!("{:02X}", s.signature.opcode.unwrap_or(0xFF)))
                    .collect::<Vec<_>>()
                    .join(" ");
                writeln!(
                    self.writer,
                    "loop x{}: [{}] cycles={}..{} pc_span={:04X}->{:04X}",
                    state.repetitions, body, start, end, first_pc, last_pc
                )?;
            } else if state.repetitions > 1 && emit_semantic {
                let start = state.window.first().map(|s| s.start_cycle).unwrap_or(0);
                let single_window_end = state.window.last().map(|s| s.end_cycle).unwrap_or(start);
                let single_window_cycles = single_window_end.saturating_sub(start);
                let cycles_consumed = single_window_cycles.saturating_mul(state.repetitions);
                writeln!(
                    self.writer,
                    "loop type=wait_ly_vblank iterations={} cycles={} ly_range={:02X}..{:02X} ly_end={:02X}",
                    state.repetitions,
                    cycles_consumed,
                    state.ly_observed_min,
                    state.ly_observed_max,
                    state.ly_terminating
                )?;
            } else {
                for step in state.window {
                    writeln!(self.writer, "{}", step.text)?;
                }
            }
        }
        Ok(())
    }

    fn finalize(&mut self) -> io::Result<()> {
        self.flush_loop_summary()?;
        self.flush_pending_raw()
    }
}

fn summarize_wait_loop(window: &[PendingStep]) -> Option<LoopSummary> {
    if window.len() < 2 {
        return None;
    }
    let mut saw_ly_read = false;
    let mut saw_conditional_back_jump = false;
    let mut high_confidence = false;
    let mut saw_compare = false;
    for step in window {
        let sig = step.signature;
        match (sig.opcode, sig.operand1, sig.operand2) {
            (Some(0xF0), Some(0x44), _) => {
                saw_ly_read = true;
                high_confidence = true;
            }
            (Some(0xFA), Some(0x44), Some(0xFF)) => {
                saw_ly_read = true;
                high_confidence = true;
            }
            _ => {}
        }
        if matches!(sig.opcode, Some(0x20 | 0x28 | 0x30 | 0x38))
            && sig.branch_taken
            && sig.pc_after < sig.pc_before
        {
            saw_conditional_back_jump = true;
        }
        if sig.opcode == Some(0xFE) && sig.operand1.is_some() {
            saw_compare = true;
        }
    }

    if !(saw_ly_read && saw_conditional_back_jump) {
        return None;
    }

    Some(LoopSummary {
        kind: LoopKind::WaitLyVblank,
        confidence_high: high_confidence && saw_compare,
        has_compare_value: saw_compare,
    })
}

fn ly_range_for_steps(steps: &[PendingStep]) -> Option<(u8, u8, u8)> {
    let mut ly_min = u8::MAX;
    let mut ly_max = u8::MIN;
    for step in steps {
        ly_min = ly_min.min(step.signature.ppu_ly_before);
        ly_min = ly_min.min(step.signature.ppu_ly_after);
        ly_max = ly_max.max(step.signature.ppu_ly_before);
        ly_max = ly_max.max(step.signature.ppu_ly_after);
    }
    let ly_terminating = steps.last()?.signature.ppu_ly_after;
    Some((ly_min, ly_max, ly_terminating))
}

fn merge_ly_range(current: (u8, u8, u8), next: (u8, u8, u8)) -> (u8, u8, u8) {
    (current.0.min(next.0), current.1.max(next.1), next.2)
}

impl<'a> EmulatorObserver for TraceCollector<'a> {
    fn on_event(&mut self, event: EmulatorEvent) {
        if self.exit_reason.is_some() || self.io_error.is_some() {
            return;
        }

        match event {
            EmulatorEvent::CpuStep(observation) => {
                if let Some(limit) = self.config.max_steps {
                    if self.budget_steps >= limit {
                        self.exit_reason = Some(ExitReason::MaxStepsReached { limit });
                        return;
                    }
                }

                let line = format_cpu_step_line(self.cpu_steps, &observation, self.config.format);
                let pending = PendingStep {
                    step_index: self.cpu_steps,
                    text: line,
                    start_cycle: observation.start_cycle,
                    end_cycle: observation.end_cycle,
                    signature: StepSignature::from_observation(&observation),
                };
                self.pending_steps.push_back(pending);
                if let Err(error) = update_loop_compression(self) {
                    self.io_error = Some(error);
                    return;
                }
                self.cpu_steps = self.cpu_steps.saturating_add(1);
                self.budget_steps = self.budget_steps.saturating_add(1);
                self.executed_cycles = observation.end_cycle;
                self.last_cpu_pc_before = Some(observation.pc_before);

                if self.exit_reason.is_none() {
                    self.exit_reason = exit_reason_from_step(self.config, &observation);
                }
                if self.exit_reason.is_none() && self.config.exit_on_unimplemented {
                    if let Some(opcode) = observation.unimplemented_opcode {
                        self.exit_reason = Some(ExitReason::UnimplementedOpcode {
                            opcode,
                            pc: observation.pc_before,
                        });
                    }
                }
                if self.exit_reason.is_none() {
                    if let Some(limit) = self.config.max_cycles {
                        if observation.end_cycle >= limit {
                            self.exit_reason = Some(ExitReason::MaxCyclesReached { limit });
                        }
                    }
                }
            }
            EmulatorEvent::HaltedFastForward(observation) => {
                if let Err(error) = self
                    .flush_loop_summary()
                    .and_then(|_| self.flush_pending_raw())
                {
                    self.io_error = Some(error);
                    return;
                }
                if let Some(limit) = self.config.max_steps {
                    if self.budget_steps >= limit {
                        self.exit_reason = Some(ExitReason::MaxStepsReached { limit });
                        return;
                    }
                }
                if let Err(error) = write_halted_fast_forward_line(self.writer, &observation) {
                    self.io_error = Some(error);
                    return;
                }
                self.budget_steps = self.budget_steps.saturating_add(1);
                self.executed_cycles = observation.end_cycle;
                if self.exit_reason.is_none() {
                    if let Some(limit) = self.config.max_cycles {
                        if observation.end_cycle >= limit {
                            self.exit_reason = Some(ExitReason::MaxCyclesReached { limit });
                        }
                    }
                }
            }
            EmulatorEvent::WatchIo(observation) => {
                if let Err(error) = self
                    .flush_loop_summary()
                    .and_then(|_| self.flush_pending_raw())
                {
                    self.io_error = Some(error);
                    return;
                }
                if let Err(error) = write_watch_io_line(self.writer, &observation) {
                    self.io_error = Some(error);
                }
            }
        }
    }

    fn should_stop(&self) -> bool {
        self.exit_reason.is_some() || self.io_error.is_some()
    }
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

fn parse_cli() -> Result<CliParseResult, UsageError> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        return Ok(CliParseResult::Help);
    }
    if args.len() < 2 {
        return Err(UsageError(
            "missing ROM path and/or output trace path".to_string(),
        ));
    }
    let rom_path = PathBuf::from(args.remove(0));
    let output_path = PathBuf::from(args.remove(0));
    let mut args = args.into_iter();

    let mut cycle_step = DEFAULT_CYCLE_STEP;
    let mut max_steps = None;
    let mut max_cycles = None;
    let mut exit_on_jr_fe = true;
    let mut exit_on_unimplemented = true;
    let mut watch_io = false;
    let mut format = TraceFormat::Normal;
    let mut summarize_waits_override = None;

    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--cycle-step" => {
                let value = args
                    .next()
                    .ok_or_else(|| UsageError("missing value for --cycle-step".to_string()))?;
                cycle_step = parse_u32(&value, "cycle-step")?;
                if cycle_step == 0 {
                    return Err(UsageError(
                        "--cycle-step must be greater than zero".to_string(),
                    ));
                }
            }
            "--max-steps" => {
                let value = args
                    .next()
                    .ok_or_else(|| UsageError("missing value for --max-steps".to_string()))?;
                max_steps = Some(parse_u64(&value, "max-steps")?);
            }
            "--max-cycles" => {
                let value = args
                    .next()
                    .ok_or_else(|| UsageError("missing value for --max-cycles".to_string()))?;
                max_cycles = Some(parse_u64(&value, "max-cycles")?);
            }
            "--exit-on-jr-fe" => exit_on_jr_fe = true,
            "--no-exit-on-jr-fe" => exit_on_jr_fe = false,
            "--exit-on-unimplemented" => exit_on_unimplemented = true,
            "--no-exit-on-unimplemented" => exit_on_unimplemented = false,
            "--watch-io" => watch_io = true,
            "--summarize-waits" => summarize_waits_override = Some(true),
            "--no-summarize-waits" => summarize_waits_override = Some(false),
            "--format" => {
                let value = args
                    .next()
                    .ok_or_else(|| UsageError("missing value for --format".to_string()))?;
                format = TraceFormat::parse(&value)?;
            }
            "-h" | "--help" => {
                return Ok(CliParseResult::Help);
            }
            _ => {
                return Err(UsageError(format!(
                    "unrecognized argument '{flag}'\n{}",
                    usage()
                )));
            }
        }
    }

    let summarize_waits = summarize_waits_override.unwrap_or(match format {
        TraceFormat::Full => false,
        TraceFormat::Minimal | TraceFormat::Normal => true,
    });
    let summarize_waits_overridden = summarize_waits_override.is_some();

    Ok(CliParseResult::Config(CliConfig {
        rom_path,
        output_path,
        cycle_step,
        max_steps,
        max_cycles,
        exit_on_jr_fe,
        exit_on_unimplemented,
        watch_io,
        format,
        summarize_waits,
        summarize_waits_overridden,
    }))
}

fn usage() -> String {
    "usage: trace_rom <path-to-rom.gb> <trace-output.txt> [--max-steps N] [--max-cycles N] [--cycle-step N] [--watch-io] [--format minimal|normal|full] [--summarize-waits|--no-summarize-waits] [--exit-on-jr-fe|--no-exit-on-jr-fe] [--exit-on-unimplemented|--no-exit-on-unimplemented]\n\
--summarize-waits aggregates canonical LY polling loops (FF44 + conditional backward jump) into one semantic event.\n\
Default: on for minimal/normal format, off for full format."
        .to_string()
}

fn load_emulator(rom_path: &PathBuf) -> Result<Emulator, String> {
    let rom_data = fs::read(rom_path)
        .map_err(|error| format!("failed to read ROM '{}': {error}", rom_path.display()))?;

    let cartridge = Cartridge::from_rom(rom_data).map_err(|error| {
        format!(
            "failed to parse cartridge from ROM '{}': {error:?}",
            rom_path.display()
        )
    })?;

    Ok(Emulator::from_cartridge(cartridge))
}

fn format_cpu_step_line(
    step_index: u64,
    observation: &CpuStepObservation,
    format: TraceFormat,
) -> String {
    match format {
        TraceFormat::Minimal => format_cpu_step_line_minimal(step_index, observation),
        TraceFormat::Normal => format_cpu_step_line_normal(step_index, observation),
        TraceFormat::Full => format_cpu_step_line_full(step_index, observation),
    }
}

fn format_operand(value: Option<u8>) -> String {
    value
        .map(|byte| format!("{byte:02X}"))
        .unwrap_or_else(|| "--".to_string())
}

fn format_opcode(value: Option<u8>) -> String {
    value
        .map(|opcode| format!("{opcode:02X}"))
        .unwrap_or_else(|| "--".to_string())
}

fn format_cpu_step_line_minimal(step_index: u64, observation: &CpuStepObservation) -> String {
    let mut changed = Vec::new();
    let before = &observation.registers_before;
    let after = &observation.registers_after;
    if before.a != after.a {
        changed.push(format!("a={:02X}", after.a));
    }
    if before.f != after.f {
        changed.push(format!("f={:02X}", after.f));
    }
    if before.b != after.b {
        changed.push(format!("b={:02X}", after.b));
    }
    if before.c != after.c {
        changed.push(format!("c={:02X}", after.c));
    }
    if before.d != after.d {
        changed.push(format!("d={:02X}", after.d));
    }
    if before.e != after.e {
        changed.push(format!("e={:02X}", after.e));
    }
    if before.h != after.h {
        changed.push(format!("h={:02X}", after.h));
    }
    if before.l != after.l {
        changed.push(format!("l={:02X}", after.l));
    }
    if observation.sp_before != observation.sp_after {
        changed.push(format!("sp={:04X}", observation.sp_after));
    }
    if observation.ime_before != observation.ime_after {
        changed.push(format!("ime={}", observation.ime_after));
    }
    if observation.halted_before != observation.halted_after {
        changed.push(format!("halted={}", observation.halted_after));
    }

    format!(
        "step={step_index} cycles={}..{} pc={:04X}->{:04X} opcode={} bytes=[{} {}]{}",
        observation.start_cycle,
        observation.end_cycle,
        observation.pc_before,
        observation.pc_after,
        format_opcode(observation.opcode_hint),
        format_operand(observation.operand1_before),
        format_operand(observation.operand2_before),
        if changed.is_empty() {
            "".to_string()
        } else {
            format!(" {}", changed.join(" "))
        },
    )
}

fn format_cpu_step_line_normal(step_index: u64, observation: &CpuStepObservation) -> String {
    let regs = &observation.registers_after;
    format!(
        "step={step_index} cycles={}..{} pc={:04X}->{:04X} opcode={} bytes=[{} {}] a={:02X} f={:02X} b={:02X} c={:02X} d={:02X} e={:02X} h={:02X} l={:02X} sp={:04X} ime={} halted={} ppu_lcdc={:02X}->{:02X} ppu_stat={:02X}->{:02X} ppu_ly={:02X}->{:02X}",
        observation.start_cycle,
        observation.end_cycle,
        observation.pc_before,
        observation.pc_after,
        format_opcode(observation.opcode_hint),
        format_operand(observation.operand1_before),
        format_operand(observation.operand2_before),
        regs.a,
        regs.f,
        regs.b,
        regs.c,
        regs.d,
        regs.e,
        regs.h,
        regs.l,
        observation.sp_after,
        observation.ime_after,
        observation.halted_after,
        observation.ppu_before.lcdc,
        observation.ppu_after.lcdc,
        observation.ppu_before.stat,
        observation.ppu_after.stat,
        observation.ppu_before.ly,
        observation.ppu_after.ly,
    )
}

fn format_cpu_step_line_full(step_index: u64, observation: &CpuStepObservation) -> String {
    let regs = &observation.registers_after;
    format!(
        "step={step_index} cycles={}..{} pc={:04X}->{:04X} opcode={} bytes=[{} {}] a={:02X} f={:02X} b={:02X} c={:02X} d={:02X} e={:02X} h={:02X} l={:02X} sp={:04X} ime={} halted={} ppu_lcdc={:02X}->{:02X} ppu_stat={:02X}->{:02X} ppu_ly={:02X}->{:02X} ppu_lyc={:02X}->{:02X} ppu_dot={:03}->{:03} ppu_lcd_warmup={}->{}",
        observation.start_cycle,
        observation.end_cycle,
        observation.pc_before,
        observation.pc_after,
        format_opcode(observation.opcode_hint),
        format_operand(observation.operand1_before),
        format_operand(observation.operand2_before),
        regs.a,
        regs.f,
        regs.b,
        regs.c,
        regs.d,
        regs.e,
        regs.h,
        regs.l,
        observation.sp_after,
        observation.ime_after,
        observation.halted_after,
        observation.ppu_before.lcdc,
        observation.ppu_after.lcdc,
        observation.ppu_before.stat,
        observation.ppu_after.stat,
        observation.ppu_before.ly,
        observation.ppu_after.ly,
        observation.ppu_before.lyc,
        observation.ppu_after.lyc,
        observation.ppu_before.scanline_dot,
        observation.ppu_after.scanline_dot,
        observation.ppu_before.lcd_enable_delay_dots,
        observation.ppu_after.lcd_enable_delay_dots,
    )
}

fn write_halted_fast_forward_line(
    writer: &mut BufWriter<fs::File>,
    observation: &HaltedFastForwardObservation,
) -> io::Result<()> {
    writeln!(
        writer,
        "halt-fast-forward cycles={}..{} pc={:04X} advanced={}",
        observation.start_cycle, observation.end_cycle, observation.pc, observation.cycles,
    )
}

fn write_watch_io_line(
    writer: &mut BufWriter<fs::File>,
    observation: &WatchIoObservation,
) -> io::Result<()> {
    let opcode = observation
        .opcode_hint
        .map(|opcode| format!("{opcode:02X}"))
        .unwrap_or_else(|| "--".to_string());
    let access = match observation.access_type {
        WatchIoAccessType::Read => "read",
        WatchIoAccessType::Write => "write",
    };

    writeln!(
        writer,
        "watch-io cycle={} pc={:04X} opcode={} type={} addr={:04X} value={:02X} ppu_mode={} ppu_coincidence={}",
        observation.step_start_cycle,
        observation.pc,
        opcode,
        access,
        observation.address,
        observation.value,
        observation.ppu_mode,
        observation.ppu_coincidence,
    )
}

fn exit_reason_from_step(
    config: &CliConfig,
    observation: &CpuStepObservation,
) -> Option<ExitReason> {
    let pending_interrupts =
        observation.interrupt_enable & observation.interrupt_flag & interrupts::MASK;

    if config.exit_on_jr_fe
        && observation.opcode_hint == Some(0x18)
        && observation.pc_after == observation.pc_before
        && !(observation.ime_after && pending_interrupts != 0)
    {
        return Some(ExitReason::JrFeInfiniteLoop {
            pc: observation.pc_before,
        });
    }

    None
}

fn cycle_batch_target(config: &CliConfig, steps: u64, total_cycles: u64) -> u32 {
    let mut target = config.cycle_step;

    if let Some(limit) = config.max_steps {
        let remaining_steps = limit.saturating_sub(steps);
        if remaining_steps == 0 {
            return 0;
        }
        let max_cycles_for_remaining_steps = remaining_steps.saturating_mul(4);
        target = target.min(max_cycles_for_remaining_steps.min(u64::from(u32::MAX)) as u32);
    }

    if let Some(limit) = config.max_cycles {
        let remaining_cycles = limit.saturating_sub(total_cycles);
        if remaining_cycles == 0 {
            return 0;
        }
        target = target.min(remaining_cycles.min(u64::from(u32::MAX)) as u32);
    }

    target.max(1)
}

fn main() -> ExitCode {
    let config = match parse_cli() {
        Ok(CliParseResult::Help) => {
            println!("{}", usage());
            return ExitCode::SUCCESS;
        }
        Ok(CliParseResult::Config(config)) => config,
        Err(error) => {
            eprintln!("error: {error}");
            eprintln!("{}", usage());
            return ExitCode::FAILURE;
        }
    };

    let mut emulator = match load_emulator(&config.rom_path) {
        Ok(emulator) => emulator,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    };
    emulator.set_watch_io_enabled(config.watch_io);

    let trace_file = match fs::File::create(&config.output_path) {
        Ok(file) => file,
        Err(error) => {
            eprintln!(
                "error: failed to create trace file '{}': {error}",
                config.output_path.display()
            );
            return ExitCode::FAILURE;
        }
    };
    let mut trace_writer = BufWriter::new(trace_file);

    let mut observer = TraceCollector::new(&mut trace_writer, &config);

    while observer.exit_reason.is_none() {
        if let Some(limit) = config.max_steps {
            if observer.budget_steps >= limit {
                observer.exit_reason = Some(ExitReason::MaxStepsReached { limit });
                break;
            }
        }

        if let Some(limit) = config.max_cycles {
            if emulator.total_cycles() >= limit {
                observer.exit_reason = Some(ExitReason::MaxCyclesReached { limit });
                break;
            }
        }

        let cycle_batch =
            cycle_batch_target(&config, observer.budget_steps, emulator.total_cycles());
        if cycle_batch == 0 {
            observer.exit_reason = Some(if let Some(limit) = config.max_steps {
                ExitReason::MaxStepsReached { limit }
            } else {
                ExitReason::MaxCyclesReached {
                    limit: config.max_cycles.unwrap_or(emulator.total_cycles()),
                }
            });
            break;
        }

        emulator.step_cycles_with_observer(cycle_batch, &mut observer);

        if let Some(error) = observer.io_error {
            eprintln!(
                "error: failed writing trace to '{}': {error}",
                config.output_path.display()
            );
            return ExitCode::FAILURE;
        }
    }

    if let Err(error) = observer.finalize() {
        eprintln!(
            "error: failed writing trace to '{}': {error}",
            config.output_path.display()
        );
        return ExitCode::FAILURE;
    }
    let final_exit_reason = observer.exit_reason.take();
    let final_cycles = observer.executed_cycles;
    let final_steps = observer.cpu_steps;
    drop(observer);

    if let Err(error) = trace_writer.flush() {
        eprintln!(
            "error: failed to flush trace file '{}': {error}",
            config.output_path.display()
        );
        return ExitCode::FAILURE;
    }

    match final_exit_reason {
        Some(ExitReason::MaxStepsReached { limit }) => {
            println!("trace completed after reaching step limit ({limit})");
        }
        Some(ExitReason::MaxCyclesReached { limit }) => {
            println!(
                "trace completed after reaching cycle limit ({limit}); executed cycles={} steps={steps}",
                final_cycles,
                steps = final_steps
            );
        }
        Some(ExitReason::JrFeInfiniteLoop { pc }) => {
            println!(
                "trace completed: detected infinite loop via JR -2 at PC={pc:04X}; cycles={} steps={steps}",
                final_cycles,
                steps = final_steps
            );
        }
        Some(ExitReason::UnimplementedOpcode { opcode, pc }) => {
            println!(
                "trace completed: hit unimplemented opcode {opcode:02X} at PC={pc:04X}; cycles={} steps={steps}",
                final_cycles,
                steps = final_steps
            );
        }
        None => {
            println!(
                "trace completed without explicit exit condition; cycles={} steps={steps}",
                final_cycles,
                steps = final_steps
            );
        }
    }

    ExitCode::SUCCESS
}

fn update_loop_compression(collector: &mut TraceCollector<'_>) -> io::Result<()> {
    if let Some(state) = collector.loop_state.as_mut() {
        let size = state.window.len();
        let loop_start_step = state
            .window
            .first()
            .map(|step| step.step_index)
            .unwrap_or(u64::MAX);
        if collector.pending_steps.len() >= size {
            let tail = collector
                .pending_steps
                .range(collector.pending_steps.len() - size..)
                .cloned()
                .collect::<Vec<_>>();
            if tail
                .first()
                .is_some_and(|step| step.step_index < loop_start_step)
            {
                return Ok(());
            }
            if state
                .window
                .iter()
                .zip(&tail)
                .all(|(x, y)| x.signature == y.signature && !y.signature.interrupt_entry)
            {
                state.repetitions = state.repetitions.saturating_add(1);
                if let Some(tail_range) = ly_range_for_steps(&tail) {
                    let merged = merge_ly_range(
                        (
                            state.ly_observed_min,
                            state.ly_observed_max,
                            state.ly_terminating,
                        ),
                        tail_range,
                    );
                    state.ly_observed_min = merged.0;
                    state.ly_observed_max = merged.1;
                    state.ly_terminating = merged.2;
                }
                collector
                    .pending_steps
                    .truncate(collector.pending_steps.len() - size);
                return Ok(());
            }
        } else {
            return Ok(());
        }
        collector.flush_loop_summary()?;
    }
    if collector.pending_steps.len() < LOOP_WINDOW_MIN * 2 {
        return Ok(());
    }
    let len = collector.pending_steps.len();
    let preferred = LOOP_WINDOW_PREFERRED.min(len / 2);
    let candidates = std::iter::once(preferred).chain(
        (LOOP_WINDOW_MIN..=LOOP_WINDOW_MAX.min(len / 2))
            .rev()
            .filter(move |s| *s != preferred),
    );
    for size in candidates {
        if size < LOOP_WINDOW_MIN {
            continue;
        }
        let a = collector
            .pending_steps
            .range(len - 2 * size..len - size)
            .cloned()
            .collect::<Vec<_>>();
        let b = collector
            .pending_steps
            .range(len - size..len)
            .cloned()
            .collect::<Vec<_>>();
        if a.iter()
            .zip(&b)
            .all(|(x, y)| x.signature == y.signature && !x.signature.interrupt_entry)
        {
            collector.pending_steps.truncate(len - 2 * size);
            let range_a = ly_range_for_steps(&a).unwrap_or((0, 0, 0));
            let range_b = ly_range_for_steps(&b).unwrap_or((0, 0, 0));
            let merged = merge_ly_range(range_a, range_b);
            collector.loop_state = Some(LoopState {
                window: a,
                repetitions: 2,
                ly_observed_min: merged.0,
                ly_observed_max: merged.1,
                ly_terminating: merged.2,
            });
            return Ok(());
        }
    }
    if collector.pending_steps.len() > LOOP_WINDOW_MAX * 2 {
        if let Some(step) = collector.pending_steps.pop_front() {
            writeln!(collector.writer, "{}", step.text)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use latchboy_core::{cpu::Registers, observability::PpuSnapshot};

    fn step(
        start_cycle: u64,
        pc_before: u16,
        pc_after: u16,
        opcode: u8,
        operand1: u8,
    ) -> CpuStepObservation {
        CpuStepObservation {
            start_cycle,
            end_cycle: start_cycle + 4,
            pc_before,
            pc_after,
            operand1_before: Some(operand1),
            operand2_before: None,
            sp_before: 0xFFFE,
            sp_after: 0xFFFE,
            opcode_hint: Some(opcode),
            cycles: 4,
            registers_before: Registers::default(),
            registers_after: Registers::default(),
            ime_before: false,
            ime_after: false,
            halted_before: false,
            halted_after: false,
            interrupt_flag_before: 0,
            interrupt_enable_before: 0,
            ppu_before: PpuSnapshot {
                lcdc: 0,
                stat: 0,
                ly: 0x44,
                lyc: 0,
                scanline_dot: 0,
                lcd_enable_delay_dots: 0,
            },
            interrupt_flag: 0,
            interrupt_enable: 0,
            ppu_after: PpuSnapshot {
                lcdc: 0,
                stat: 0,
                ly: 0x44,
                lyc: 0,
                scanline_dot: 0,
                lcd_enable_delay_dots: 0,
            },
            unimplemented_opcode: None,
        }
    }

    #[test]
    fn compresses_ly_polling_loop_and_flushes_on_exit() {
        let path = std::env::temp_dir().join("trace_rom_loop_test.txt");
        let file = fs::File::create(&path).unwrap();
        let mut writer = BufWriter::new(file);
        let config = CliConfig {
            rom_path: PathBuf::new(),
            output_path: PathBuf::new(),
            cycle_step: 1,
            max_steps: None,
            max_cycles: None,
            exit_on_jr_fe: false,
            exit_on_unimplemented: false,
            watch_io: false,
            format: TraceFormat::Minimal,
            summarize_waits: true,
            summarize_waits_overridden: false,
        };
        let mut c = TraceCollector::new(&mut writer, &config);
        let prefix = step(0, 0x0000, 0x0001, 0x00, 0x00);
        c.pending_steps.push_back(PendingStep {
            step_index: 0,
            text: format_cpu_step_line(0, &prefix, TraceFormat::Minimal),
            start_cycle: 0,
            end_cycle: 4,
            signature: StepSignature::from_observation(&prefix),
        });
        let seq = [
            step(0, 0x0100, 0x0102, 0xF0, 0x44),
            step(4, 0x0102, 0x0104, 0xFE, 0x90),
            step(8, 0x0104, 0x0100, 0x20, 0xFA),
        ];
        let mut step_index = 1u64;
        for i in 0..2 {
            for s in seq.iter() {
                let obs = CpuStepObservation {
                    start_cycle: s.start_cycle + i * 12,
                    end_cycle: s.end_cycle + i * 12,
                    ..s.clone()
                };
                c.pending_steps.push_back(PendingStep {
                    step_index,
                    text: format_cpu_step_line(step_index, &obs, TraceFormat::Minimal),
                    start_cycle: obs.start_cycle,
                    end_cycle: obs.end_cycle,
                    signature: StepSignature::from_observation(&obs),
                });
                step_index += 1;
            }
        }
        update_loop_compression(&mut c).unwrap();
        assert!(c.loop_state.is_some());
        c.pending_steps.push_back(PendingStep {
            step_index,
            text: format_cpu_step_line(
                step_index,
                &step(24, 0x0104, 0x0106, 0x20, 0xFA),
                TraceFormat::Minimal,
            ),
            start_cycle: 24,
            end_cycle: 28,
            signature: StepSignature::from_observation(&step(24, 0x0104, 0x0106, 0x20, 0xFA)),
        });
        update_loop_compression(&mut c).unwrap();
        c.finalize().unwrap();
        drop(c);
        writer.flush().unwrap();
        let out = fs::read_to_string(path).unwrap();
        assert!(out.contains("loop type=wait_ly_vblank"));
        assert!(out.contains("iterations=2"));
        assert!(out.contains("cycles=24"));
        assert!(out.contains("step="));
        assert!(out.find("step=0").unwrap() < out.find("loop type=wait_ly_vblank").unwrap());
    }

    #[test]
    fn marks_interrupt_entry_on_ime_disable_and_stack_push() {
        let mut observation = step(0, 0x0100, 0x0040, 0x00, 0x00);
        observation.ime_before = true;
        observation.ime_after = false;
        observation.sp_before = 0xFFFE;
        observation.sp_after = 0xFFFC;
        let signature = StepSignature::from_observation(&observation);
        assert!(signature.interrupt_entry);
    }

    #[test]
    fn does_not_break_active_loop_until_full_window_arrives() {
        let path = std::env::temp_dir().join("trace_rom_loop_extension_test.txt");
        let file = fs::File::create(&path).unwrap();
        let mut writer = BufWriter::new(file);
        let config = CliConfig {
            rom_path: PathBuf::new(),
            output_path: PathBuf::new(),
            cycle_step: 1,
            max_steps: None,
            max_cycles: None,
            exit_on_jr_fe: false,
            exit_on_unimplemented: false,
            watch_io: false,
            format: TraceFormat::Minimal,
            summarize_waits: true,
            summarize_waits_overridden: false,
        };
        let mut c = TraceCollector::new(&mut writer, &config);
        let loop_seq = [
            step(0, 0x0100, 0x0102, 0xF0, 0x44),
            step(4, 0x0102, 0x0104, 0xFE, 0x90),
            step(8, 0x0104, 0x0100, 0x20, 0xFA),
        ];
        c.pending_steps.push_back(PendingStep {
            step_index: 0,
            text: format_cpu_step_line(
                0,
                &step(0, 0x0000, 0x0001, 0x00, 0x00),
                TraceFormat::Minimal,
            ),
            start_cycle: 0,
            end_cycle: 4,
            signature: StepSignature::from_observation(&step(0, 0x0000, 0x0001, 0x00, 0x00)),
        });
        for (idx, s) in loop_seq.iter().chain(loop_seq.iter()).enumerate() {
            let step_index = (idx + 1) as u64;
            c.pending_steps.push_back(PendingStep {
                step_index,
                text: format_cpu_step_line(step_index, s, TraceFormat::Minimal),
                start_cycle: s.start_cycle + ((idx / 3) as u64) * 12,
                end_cycle: s.end_cycle + ((idx / 3) as u64) * 12,
                signature: StepSignature::from_observation(s),
            });
        }
        update_loop_compression(&mut c).unwrap();
        assert!(c.loop_state.is_some());

        c.pending_steps.push_back(PendingStep {
            step_index: 7,
            text: format_cpu_step_line(7, &loop_seq[0], TraceFormat::Minimal),
            start_cycle: 24,
            end_cycle: 28,
            signature: StepSignature::from_observation(&loop_seq[0]),
        });
        update_loop_compression(&mut c).unwrap();
        assert!(c.loop_state.is_some());

        c.pending_steps.push_back(PendingStep {
            step_index: 8,
            text: format_cpu_step_line(8, &loop_seq[1], TraceFormat::Minimal),
            start_cycle: 28,
            end_cycle: 32,
            signature: StepSignature::from_observation(&loop_seq[1]),
        });
        c.pending_steps.push_back(PendingStep {
            step_index: 9,
            text: format_cpu_step_line(9, &loop_seq[2], TraceFormat::Minimal),
            start_cycle: 32,
            end_cycle: 36,
            signature: StepSignature::from_observation(&loop_seq[2]),
        });
        update_loop_compression(&mut c).unwrap();
        assert_eq!(c.loop_state.as_ref().map(|s| s.repetitions), Some(3));
    }
}
