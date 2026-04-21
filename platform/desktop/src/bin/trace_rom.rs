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
    },
    Emulator,
};

const DEFAULT_CYCLE_STEP: u32 = 1;

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
        }
    }
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

                if let Err(error) = write_cpu_step_line(self.writer, self.cpu_steps, &observation) {
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
                if self.exit_reason.is_none() {
                    if let Some(limit) = self.config.max_cycles {
                        if observation.end_cycle >= limit {
                            self.exit_reason = Some(ExitReason::MaxCyclesReached { limit });
                        }
                    }
                }
            }
            EmulatorEvent::HaltedFastForward(observation) => {
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
        }
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

    Ok(CliParseResult::Config(CliConfig {
        rom_path,
        output_path,
        cycle_step,
        max_steps,
        max_cycles,
        exit_on_jr_fe,
        exit_on_unimplemented,
    }))
}

fn usage() -> String {
    "usage: trace_rom <path-to-rom.gb> <trace-output.txt> [--max-steps N] [--max-cycles N] [--cycle-step N] [--exit-on-jr-fe|--no-exit-on-jr-fe] [--exit-on-unimplemented|--no-exit-on-unimplemented]".to_string()
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

fn write_cpu_step_line(
    writer: &mut BufWriter<fs::File>,
    step_index: u64,
    observation: &CpuStepObservation,
) -> io::Result<()> {
    fn format_operand(value: Option<u8>) -> String {
        value
            .map(|byte| format!("{byte:02X}"))
            .unwrap_or_else(|| "--".to_string())
    }

    let regs = &observation.registers_after;
    let opcode = observation
        .opcode_hint
        .map(|opcode| format!("{opcode:02X}"))
        .unwrap_or_else(|| "--".to_string());
    let operand1 = format_operand(observation.operand1_before);
    let operand2 = format_operand(observation.operand2_before);

    writeln!(
        writer,
        "step={step_index} cycles={}..{} pc={:04X}->{:04X} opcode={} bytes=[{} {}] a={:02X} f={:02X} b={:02X} c={:02X} d={:02X} e={:02X} h={:02X} l={:02X} sp={:04X} ime={} halted={}",
        observation.start_cycle,
        observation.end_cycle,
        observation.pc_before,
        observation.pc_after,
        opcode,
        operand1,
        operand2,
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

fn exit_reason_from_step(
    config: &CliConfig,
    observation: &CpuStepObservation,
) -> Option<ExitReason> {
    let pending_enabled_interrupts =
        observation.interrupt_flag & observation.interrupt_enable & interrupts::MASK;

    if config.exit_on_jr_fe
        && observation.opcode_hint == Some(0x18)
        && observation.pc_after == observation.pc_before
        && !(observation.ime_after && pending_enabled_interrupts != 0)
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

    let mut cpu_steps = 0u64;
    let mut budget_steps = 0u64;
    let mut executed_cycles = 0u64;
    let mut exit_reason = None;
    let mut last_cpu_pc_before = None;

    while exit_reason.is_none() {
        if let Some(limit) = config.max_steps {
            if budget_steps >= limit {
                exit_reason = Some(ExitReason::MaxStepsReached { limit });
                break;
            }
        }

        if let Some(limit) = config.max_cycles {
            if emulator.total_cycles() >= limit {
                exit_reason = Some(ExitReason::MaxCyclesReached { limit });
                break;
            }
        }

        let cycle_batch = cycle_batch_target(&config, budget_steps, emulator.total_cycles());
        if cycle_batch == 0 {
            exit_reason = Some(if let Some(limit) = config.max_steps {
                ExitReason::MaxStepsReached { limit }
            } else {
                ExitReason::MaxCyclesReached {
                    limit: config.max_cycles.unwrap_or(emulator.total_cycles()),
                }
            });
            break;
        }

        let mut observer = TraceCollector::new(&mut trace_writer, &config);
        observer.cpu_steps = cpu_steps;
        observer.budget_steps = budget_steps;
        observer.executed_cycles = executed_cycles;
        observer.last_cpu_pc_before = last_cpu_pc_before;
        emulator.step_cycles_with_observer(cycle_batch, &mut observer);

        if let Some(error) = observer.io_error {
            eprintln!(
                "error: failed writing trace to '{}': {error}",
                config.output_path.display()
            );
            return ExitCode::FAILURE;
        }

        cpu_steps = observer.cpu_steps;
        budget_steps = observer.budget_steps;
        executed_cycles = observer.executed_cycles;
        last_cpu_pc_before = observer.last_cpu_pc_before;
        exit_reason = observer.exit_reason;

        if exit_reason.is_none() && config.exit_on_unimplemented {
            if let Some(opcode) = emulator.cpu().last_unimplemented_opcode() {
                exit_reason = Some(ExitReason::UnimplementedOpcode {
                    opcode,
                    pc: last_cpu_pc_before.unwrap_or(emulator.cpu().pc().wrapping_sub(1)),
                });
            }
        }
    }

    if let Err(error) = trace_writer.flush() {
        eprintln!(
            "error: failed to flush trace file '{}': {error}",
            config.output_path.display()
        );
        return ExitCode::FAILURE;
    }

    match exit_reason {
        Some(ExitReason::MaxStepsReached { limit }) => {
            println!("trace completed after reaching step limit ({limit})");
        }
        Some(ExitReason::MaxCyclesReached { limit }) => {
            println!(
                "trace completed after reaching cycle limit ({limit}); executed cycles={} steps={steps}",
                executed_cycles,
                steps = cpu_steps
            );
        }
        Some(ExitReason::JrFeInfiniteLoop { pc }) => {
            println!(
                "trace completed: detected infinite loop via JR -2 at PC={pc:04X}; cycles={} steps={steps}",
                executed_cycles,
                steps = cpu_steps
            );
        }
        Some(ExitReason::UnimplementedOpcode { opcode, pc }) => {
            println!(
                "trace completed: hit unimplemented opcode {opcode:02X} at PC={pc:04X}; cycles={} steps={steps}",
                executed_cycles,
                steps = cpu_steps
            );
        }
        None => {
            println!(
                "trace completed without explicit exit condition; cycles={} steps={steps}",
                executed_cycles,
                steps = cpu_steps
            );
        }
    }

    ExitCode::SUCCESS
}
