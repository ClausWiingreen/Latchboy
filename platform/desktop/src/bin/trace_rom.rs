use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use latchboy_core::{
    cartridge::Cartridge,
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

#[derive(Debug)]
enum ExitReason {
    MaxStepsReached { limit: u64 },
    MaxCyclesReached { limit: u64 },
    JrFeInfiniteLoop { pc: u16 },
    UnimplementedOpcode { opcode: u8, pc: u16 },
}

struct TraceCollector {
    events: Vec<EmulatorEvent>,
}

impl TraceCollector {
    fn new() -> Self {
        Self { events: Vec::new() }
    }

    fn len(&self) -> usize {
        self.events.len()
    }

    fn event_at(&self, index: usize) -> Option<&EmulatorEvent> {
        self.events.get(index)
    }
}

impl EmulatorObserver for TraceCollector {
    fn on_event(&mut self, event: EmulatorEvent) {
        self.events.push(event);
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

fn parse_cli() -> Result<CliConfig, UsageError> {
    let mut args = env::args().skip(1);
    let rom_path = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| UsageError("missing ROM path".to_string()))?;
    let output_path = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| UsageError("missing output trace path".to_string()))?;

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
                return Err(UsageError(usage()));
            }
            _ => {
                return Err(UsageError(format!(
                    "unrecognized argument '{flag}'\n{}",
                    usage()
                )));
            }
        }
    }

    Ok(CliConfig {
        rom_path,
        output_path,
        cycle_step,
        max_steps,
        max_cycles,
        exit_on_jr_fe,
        exit_on_unimplemented,
    })
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
    emulator: &Emulator,
) -> io::Result<()> {
    let regs = &observation.registers_after;
    let opcode = observation
        .opcode_hint
        .map(|opcode| format!("{opcode:02X}"))
        .unwrap_or_else(|| "--".to_string());
    let op1 = emulator.bus().read8(observation.pc_before.wrapping_add(1));
    let op2 = emulator.bus().read8(observation.pc_before.wrapping_add(2));

    writeln!(
        writer,
        "step={step_index} cycles={}..{} pc={:04X}->{:04X} opcode={} bytes=[{:02X} {:02X}] a={:02X} f={:02X} b={:02X} c={:02X} d={:02X} e={:02X} h={:02X} l={:02X} sp={:04X} ime={} halted={}",
        observation.start_cycle,
        observation.end_cycle,
        observation.pc_before,
        observation.pc_after,
        opcode,
        op1,
        op2,
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
    if config.exit_on_jr_fe
        && observation.opcode_hint == Some(0x18)
        && observation.pc_after == observation.pc_before
    {
        return Some(ExitReason::JrFeInfiniteLoop {
            pc: observation.pc_before,
        });
    }

    None
}

fn main() -> ExitCode {
    let config = match parse_cli() {
        Ok(config) => config,
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

    let mut observer = TraceCollector::new();
    let mut emitted_event_index = 0usize;
    let mut steps = 0u64;
    let mut exit_reason = None;

    while exit_reason.is_none() {
        if let Some(limit) = config.max_steps {
            if steps >= limit {
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

        let events_before = observer.len();
        emulator.step_cycles_with_observer(config.cycle_step, &mut observer);
        if observer.len() == events_before {
            continue;
        }

        while emitted_event_index < observer.len() {
            let Some(event) = observer.event_at(emitted_event_index) else {
                break;
            };

            match event {
                EmulatorEvent::CpuStep(observation) => {
                    if let Err(error) =
                        write_cpu_step_line(&mut trace_writer, steps, observation, &emulator)
                    {
                        eprintln!(
                            "error: failed writing CPU trace to '{}': {error}",
                            config.output_path.display()
                        );
                        return ExitCode::FAILURE;
                    }
                    steps = steps.saturating_add(1);
                    if exit_reason.is_none() {
                        exit_reason = exit_reason_from_step(&config, observation);
                    }
                }
                EmulatorEvent::HaltedFastForward(observation) => {
                    if let Err(error) =
                        write_halted_fast_forward_line(&mut trace_writer, observation)
                    {
                        eprintln!(
                            "error: failed writing HALT trace to '{}': {error}",
                            config.output_path.display()
                        );
                        return ExitCode::FAILURE;
                    }
                }
            }

            emitted_event_index += 1;
            if exit_reason.is_some() {
                break;
            }
        }

        if config.exit_on_unimplemented {
            if let Some(opcode) = emulator.cpu().last_unimplemented_opcode() {
                exit_reason = Some(ExitReason::UnimplementedOpcode {
                    opcode,
                    pc: emulator.cpu().pc(),
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
                emulator.total_cycles()
            );
        }
        Some(ExitReason::JrFeInfiniteLoop { pc }) => {
            println!(
                "trace completed: detected infinite loop via JR -2 at PC={pc:04X}; cycles={} steps={steps}",
                emulator.total_cycles()
            );
        }
        Some(ExitReason::UnimplementedOpcode { opcode, pc }) => {
            println!(
                "trace completed: hit unimplemented opcode {opcode:02X} at PC={pc:04X}; cycles={} steps={steps}",
                emulator.total_cycles()
            );
        }
        None => {
            println!(
                "trace completed without explicit exit condition; cycles={} steps={steps}",
                emulator.total_cycles()
            );
        }
    }

    ExitCode::SUCCESS
}
