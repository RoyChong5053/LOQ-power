mod privilege;

use std::collections::BTreeMap;
use std::io::Read;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use loq_core::profile::{self, Profile, ProfileFile};
use loq_core::request::{
    ApplyRequest, ApplyResponse, BatteryRequest, CpuRequest, IdeapadRequest, LedRequest,
    LegionRequest,
};
use loq_core::{
    Capabilities, Sysfs, battery, cpu, ec, ideapad, leds, legion, platform, probe, takeover,
    tlp_import,
};

use crate::privilege::execute;

#[derive(Debug, Parser)]
#[command(
    name = "loq-power",
    version,
    about = "LOQ Power — EC and power management for Lenovo LOQ laptops"
)]
struct Cli {
    /// Print machine-readable JSON
    #[arg(long, global = true)]
    json: bool,

    /// Validate and show the change without writing anything
    #[arg(long, global = true)]
    dry_run: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Discover every EC/power capability exposed by the kernel
    Probe,
    /// Show a short status summary
    Status,
    /// Read or write EC firmware attributes
    Ec {
        #[command(subcommand)]
        command: EcCommand,
    },
    /// Read or switch the thermal/platform profile
    Platform {
        #[command(subcommand)]
        command: PlatformCommand,
    },
    /// Read or change the battery charge type
    Charge {
        #[command(subcommand)]
        command: ChargeCommand,
    },
    /// Read or change the EC fan mode
    Fan {
        #[command(subcommand)]
        command: FanCommand,
    },
    /// Read or change CPU frequency settings
    Cpu {
        #[command(subcommand)]
        command: CpuCommand,
    },
    /// Read or change platform LEDs
    Led {
        #[command(subcommand)]
        command: LedCommand,
    },
    /// List, inspect and apply power profiles
    Profile {
        #[command(subcommand)]
        command: ProfileCommand,
    },
    /// Optional LenovoLegionLinux controls (only when legion_hwmon exists)
    Legion {
        #[command(subcommand)]
        command: LegionCommand,
    },
    /// Inspect or import the TLP configuration
    Tlp {
        #[command(subcommand)]
        command: TlpCommand,
    },
    /// Hand power management over from TLP (post-install wizard)
    Takeover {
        /// Keep the existing /etc/loq-power/profiles.toml
        #[arg(long)]
        no_import: bool,
        /// Overwrite an earlier rollback record
        #[arg(long)]
        force: bool,
    },
    /// Give power management back to TLP
    RestoreTlp,
    /// Apply a full JSON request (advanced / daemon use)
    Apply {
        /// Read the request from a file instead of stdin
        #[arg(long)]
        file: Option<std::path::PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum EcCommand {
    /// List all discovered EC attributes
    List,
    /// Show a single attribute
    Get { name: String },
    /// Write a single attribute (enters CUSTOM mode automatically)
    Set { name: String, value: i64 },
}

#[derive(Debug, Subcommand)]
enum PlatformCommand {
    Get,
    Set { mode: String },
}

#[derive(Debug, Subcommand)]
enum ChargeCommand {
    Get,
    Set { charge_type: String },
}

#[derive(Debug, Subcommand)]
enum FanCommand {
    Mode {
        #[command(subcommand)]
        command: FanModeCommand,
    },
}

#[derive(Debug, Subcommand)]
enum FanModeCommand {
    Get,
    /// Modes: 0 silent, 1 standard, 2 dust cleaning, 4 full speed
    Set {
        mode: i64,
        /// Required for the loud modes 2 and 4
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
enum CpuCommand {
    Get,
    Set {
        #[arg(long)]
        governor: Option<String>,
        #[arg(long)]
        epp: Option<String>,
        /// Minimum frequency in MHz
        #[arg(long)]
        min_mhz: Option<u64>,
        /// Maximum frequency in MHz
        #[arg(long)]
        max_mhz: Option<u64>,
        #[arg(long, conflicts_with = "no_boost")]
        boost: bool,
        #[arg(long)]
        no_boost: bool,
    },
}

#[derive(Debug, Subcommand)]
enum LedCommand {
    List,
    Set { name: String, brightness: i64 },
}

#[derive(Debug, Subcommand)]
enum LegionCommand {
    /// Show LLL sensors, power mode and fan curve
    Status,
    /// Toggle the firmware fan full-speed switch
    Fullspeed {
        #[arg(value_parser = clap::value_parser!(bool))]
        enabled: bool,
    },
    /// Set the LLL power mode (0 balanced, 1 performance, 2 quiet)
    Powermode { mode: i64 },
    /// Read, save or write fan curves
    Curve {
        #[command(subcommand)]
        command: CurveCommand,
    },
}

#[derive(Debug, Subcommand)]
enum CurveCommand {
    /// Print the current curve as TOML
    Show,
    /// Save the current curve to a file
    Save { file: std::path::PathBuf },
    /// Write a curve from a file
    Apply { file: std::path::PathBuf },
}

#[derive(Debug, Subcommand)]
enum TlpCommand {
    /// Show what the TLP configuration would turn into
    Import {
        /// Write the result to /etc/loq-power/profiles.toml
        #[arg(long)]
        write: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ProfileCommand {
    /// List available profiles
    List,
    /// Show a profile as TOML/JSON
    Show { name: String },
    /// Resolve and apply a profile (enters CUSTOM mode as needed)
    Apply { name: String },
    /// Save a profile definition from a TOML file
    Save {
        name: String,
        file: std::path::PathBuf,
    },
    /// Delete a profile
    Delete { name: String },
    /// Print the profile file path in use
    Path,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let json = cli.json;
    let dry_run = cli.dry_run;

    match cli.command {
        Command::Probe => {
            let capabilities = probe::probe();
            if json {
                print_json(&capabilities)?;
            } else {
                print_probe(&capabilities);
            }
        }
        Command::Status => {
            let capabilities = probe::probe();
            if json {
                print_json(&capabilities)?;
            } else {
                print_status(&capabilities);
            }
        }
        Command::Ec { command } => handle_ec(command, json, dry_run)?,
        Command::Platform { command } => handle_platform(command, json, dry_run)?,
        Command::Charge { command } => handle_charge(command, json, dry_run)?,
        Command::Fan { command } => handle_fan(command, json, dry_run)?,
        Command::Cpu { command } => handle_cpu(command, json, dry_run)?,
        Command::Led { command } => handle_led(command, json, dry_run)?,
        Command::Profile { command } => handle_profile(command, json, dry_run)?,
        Command::Legion { command } => handle_legion(command, json, dry_run)?,
        Command::Tlp { command } => handle_tlp(command, json, dry_run)?,
        Command::Takeover { no_import, force } => {
            handle_takeover(!no_import, force, json, dry_run)?
        }
        Command::RestoreTlp => handle_restore_tlp(json, dry_run)?,
        Command::Apply { file } => {
            let mut input = String::new();
            match file {
                Some(path) => {
                    input = std::fs::read_to_string(&path)
                        .with_context(|| format!("cannot read {}", path.display()))?;
                }
                None => {
                    std::io::stdin()
                        .read_to_string(&mut input)
                        .context("cannot read request from stdin")?;
                }
            }

            let mut request: ApplyRequest =
                serde_json::from_str(&input).context("invalid JSON request")?;
            if dry_run {
                request.dry_run = true;
            }
            report(execute(request)?, json)?;
        }
    }

    Ok(())
}

fn handle_ec(command: EcCommand, json: bool, dry_run: bool) -> Result<()> {
    let fs = Sysfs::new();
    match command {
        EcCommand::List => {
            let info = ec::discover(&fs);
            if json {
                print_json(&info)?;
            } else if info.attributes.is_empty() {
                println!("No lenovo-wmi-other EC attributes found.");
            } else {
                println!(
                    "{:<22} {:>8} {:<4} {:<12} {:<8}",
                    "ATTRIBUTE", "VALUE", "UNIT", "RANGE", "DEFAULT"
                );
                for attribute in &info.attributes {
                    let range = match (attribute.min, attribute.max) {
                        (Some(min), Some(max)) => format!("{min}..{max}"),
                        _ => "?".to_string(),
                    };
                    println!(
                        "{:<22} {:>8} {:<4} {:<12} {:<8}",
                        attribute.name,
                        attribute
                            .current
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "?".to_string()),
                        attribute.unit,
                        range,
                        attribute
                            .default
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "-".to_string()),
                    );
                }
            }
        }
        EcCommand::Get { name } => {
            let attribute = ec::attribute(&fs, &name)
                .ok_or_else(|| anyhow::anyhow!("EC attribute {name} not found"))?;
            if json {
                print_json(&attribute)?;
            } else {
                println!(
                    "{}: {} {} [{:?}..{:?}] default {:?} step {:?}",
                    attribute.display_name,
                    attribute
                        .current
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "?".to_string()),
                    attribute.unit,
                    attribute.min,
                    attribute.max,
                    attribute.default,
                    attribute.step,
                );
            }
        }
        EcCommand::Set { name, value } => {
            let request = ApplyRequest {
                ec: BTreeMap::from([(name, value)]),
                dry_run,
                ..Default::default()
            };
            report(execute(request)?, json)?;
        }
    }
    Ok(())
}

fn handle_platform(command: PlatformCommand, json: bool, dry_run: bool) -> Result<()> {
    match command {
        PlatformCommand::Get => {
            let info = platform::detect(&Sysfs::new());
            if json {
                print_json(&info)?;
            } else {
                println!(
                    "{} (choices: {})",
                    info.current.as_deref().unwrap_or("unknown"),
                    info.choices.join(" ")
                );
            }
        }
        PlatformCommand::Set { mode } => {
            let request = ApplyRequest {
                platform: Some(mode),
                dry_run,
                ..Default::default()
            };
            report(execute(request)?, json)?;
        }
    }
    Ok(())
}

fn handle_charge(command: ChargeCommand, json: bool, dry_run: bool) -> Result<()> {
    let fs = Sysfs::new();
    match command {
        ChargeCommand::Get => {
            let Some(battery) = battery::primary(&fs) else {
                bail!("no battery found");
            };
            if json {
                print_json(&battery)?;
            } else {
                println!(
                    "{}: {} (choices: {})",
                    battery.name,
                    battery.charge_type.as_deref().unwrap_or("unknown"),
                    battery.charge_types.join(" ")
                );
            }
        }
        ChargeCommand::Set { charge_type } => {
            let request = ApplyRequest {
                battery: Some(BatteryRequest {
                    name: None,
                    charge_type,
                }),
                dry_run,
                ..Default::default()
            };
            report(execute(request)?, json)?;
        }
    }
    Ok(())
}

fn handle_fan(command: FanCommand, json: bool, dry_run: bool) -> Result<()> {
    let fs = Sysfs::new();
    match command {
        FanCommand::Mode { command } => match command {
            FanModeCommand::Get => {
                let info = ideapad::detect(&fs);
                let value = info
                    .attributes
                    .iter()
                    .find(|attribute| attribute.name == "fan_mode")
                    .and_then(|attribute| attribute.value.as_deref())
                    .and_then(|value| value.parse::<i64>().ok());

                if json {
                    print_json(&serde_json::json!({
                        "mode": value,
                        "label": value.map(ideapad::fan_mode_label),
                    }))?;
                } else if let Some(mode) = value {
                    println!("fan_mode {mode} ({})", ideapad::fan_mode_label(mode));
                } else {
                    println!("fan_mode unavailable");
                }
            }
            FanModeCommand::Set { mode, yes } => {
                if (mode == 2 || mode == 4) && !yes && !dry_run {
                    bail!(
                        "fan mode {mode} ({}) is loud; re-run with --yes to confirm",
                        ideapad::fan_mode_label(mode)
                    );
                }
                let request = ApplyRequest {
                    ideapad: Some(IdeapadRequest {
                        fan_mode: Some(mode),
                        fn_lock: None,
                    }),
                    dry_run,
                    ..Default::default()
                };
                report(execute(request)?, json)?;
            }
        },
    }
    Ok(())
}

fn handle_cpu(command: CpuCommand, json: bool, dry_run: bool) -> Result<()> {
    let fs = Sysfs::new();
    match command {
        CpuCommand::Get => {
            let info = cpu::detect(&fs);
            if json {
                print_json(&info)?;
            } else {
                println!(
                    "{} cores | {} | governor {} | EPP {} | {}..{} kHz | boost {:?}",
                    info.cpus,
                    info.driver.as_deref().unwrap_or("?"),
                    info.governor.as_deref().unwrap_or("?"),
                    info.epp.as_deref().unwrap_or("?"),
                    info.scaling_min_khz.unwrap_or(0),
                    info.scaling_max_khz.unwrap_or(0),
                    info.boost,
                );
                println!("governors: {}", info.available_governors.join(" "));
                println!("EPP:       {}", info.available_epp.join(" "));
            }
        }
        CpuCommand::Set {
            governor,
            epp,
            min_mhz,
            max_mhz,
            boost,
            no_boost,
        } => {
            let request = ApplyRequest {
                cpu: Some(CpuRequest {
                    governor,
                    epp,
                    min_khz: min_mhz.map(|value| value * 1000),
                    max_khz: max_mhz.map(|value| value * 1000),
                    boost: if boost {
                        Some(true)
                    } else if no_boost {
                        Some(false)
                    } else {
                        None
                    },
                }),
                dry_run,
                ..Default::default()
            };
            report(execute(request)?, json)?;
        }
    }
    Ok(())
}

fn handle_led(command: LedCommand, json: bool, dry_run: bool) -> Result<()> {
    let fs = Sysfs::new();
    match command {
        LedCommand::List => {
            let info = leds::detect(&fs);
            if json {
                print_json(&info)?;
            } else {
                for led in info {
                    println!(
                        "{:<32} {}/{}",
                        led.name,
                        led.brightness
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "?".to_string()),
                        led.max_brightness
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "?".to_string()),
                    );
                }
            }
        }
        LedCommand::Set { name, brightness } => {
            let request = ApplyRequest {
                led: Some(LedRequest { name, brightness }),
                dry_run,
                ..Default::default()
            };
            report(execute(request)?, json)?;
        }
    }
    Ok(())
}

fn handle_profile(command: ProfileCommand, json: bool, dry_run: bool) -> Result<()> {
    let path = profile::user_path();
    let mut file = if path.exists() {
        ProfileFile::load(&path)?
    } else {
        ProfileFile::builtin()
    };

    match command {
        ProfileCommand::List => {
            if json {
                print_json(&file)?;
            } else {
                println!("Profiles ({})", path.display());
                for name in file.names() {
                    let profile = &file.profile[&name];
                    println!(
                        "  {:<10} platform={} ec={} cpu={}",
                        name,
                        profile.platform.as_deref().unwrap_or("-"),
                        profile.ec.len(),
                        profile
                            .cpu
                            .as_ref()
                            .and_then(|cpu| cpu.governor.as_deref())
                            .unwrap_or("-"),
                    );
                }
                println!(
                    "AC → {} | battery → {} | auto_switch {}",
                    file.power.ac, file.power.battery, file.power.auto_switch
                );
            }
        }
        ProfileCommand::Show { name } => {
            let profile = file.get(&name)?;
            if json {
                print_json(profile)?;
            } else {
                println!(
                    "{}",
                    toml::to_string_pretty(profile).context("cannot render profile")?
                );
            }
        }
        ProfileCommand::Apply { name } => {
            let profile = file.get(&name)?.clone();
            let resolved = profile::resolve(&Sysfs::new(), &profile);
            for warning in &resolved.warnings {
                eprintln!("warning: {warning}");
            }
            let mut request = resolved.request;
            request.dry_run = dry_run;
            report(execute(request)?, json)?;
        }
        ProfileCommand::Save { name, file: source } => {
            let content = std::fs::read_to_string(&source)
                .with_context(|| format!("cannot read {}", source.display()))?;
            let profile: Profile = toml::from_str(&content).context("invalid profile TOML")?;
            file.profile.insert(name.clone(), profile);
            file.save(&path)?;
            println!("saved profile {name} to {}", path.display());
        }
        ProfileCommand::Delete { name } => {
            if ProfileFile::builtin().profile.contains_key(&name) {
                bail!("{name} is a built-in profile and cannot be deleted");
            }
            if file.profile.remove(&name).is_none() {
                bail!("profile {name} not found");
            }
            file.save(&path)?;
            println!("deleted profile {name}");
        }
        ProfileCommand::Path => println!("{}", path.display()),
    }

    Ok(())
}

fn handle_legion(command: LegionCommand, json: bool, dry_run: bool) -> Result<()> {
    let fs = Sysfs::new();

    match command {
        LegionCommand::Status => {
            let info = legion::detect(&fs);
            if json {
                print_json(&info)?;
            } else if !info.installed {
                println!("未检测到 LenovoLegionLinux（legion_hwmon）。");
            } else {
                println!(
                    "legion_hwmon: {}",
                    info.hwmon_path.as_deref().unwrap_or("—")
                );
                println!("powermode  : {}", info.powermode.as_deref().unwrap_or("—"));
                println!("fullspeed  : {:?}", info.fullspeed);
                for sensor in info.fans.iter().chain(info.temperatures.iter()) {
                    println!("  {:<18} {:.1} {}", sensor.label, sensor.value, sensor.unit);
                }
                if let Some(curve) = &info.fan_curve {
                    println!("fan curve  : {} points", curve.points);
                    for index in 0..curve.points as usize {
                        println!(
                            "  point {:<2} fan1={:<3} fan2={:<3} cpu={:<3} gpu={:<3} ic={:<3}",
                            index + 1,
                            curve.fan1_pwm[index],
                            curve
                                .fan2_pwm
                                .as_ref()
                                .map(|values| values[index].to_string())
                                .unwrap_or_else(|| "-".to_string()),
                            curve
                                .cpu_temp
                                .get(index)
                                .map(|value| value.to_string())
                                .unwrap_or_else(|| "-".to_string()),
                            curve
                                .gpu_temp
                                .as_ref()
                                .and_then(|values| values.get(index))
                                .map(|value| value.to_string())
                                .unwrap_or_else(|| "-".to_string()),
                            curve
                                .ic_temp
                                .as_ref()
                                .and_then(|values| values.get(index))
                                .map(|value| value.to_string())
                                .unwrap_or_else(|| "-".to_string()),
                        );
                    }
                } else {
                    println!("fan curve  : 本机未暴露 hwmon 风扇曲线");
                }
            }
        }
        LegionCommand::Fullspeed { enabled } => {
            report(
                execute(ApplyRequest {
                    legion: Some(LegionRequest {
                        fullspeed: Some(enabled),
                        ..Default::default()
                    }),
                    dry_run,
                    ..Default::default()
                })?,
                json,
            )?;
        }
        LegionCommand::Powermode { mode } => {
            report(
                execute(ApplyRequest {
                    legion: Some(LegionRequest {
                        powermode: Some(mode),
                        ..Default::default()
                    }),
                    dry_run,
                    ..Default::default()
                })?,
                json,
            )?;
        }
        LegionCommand::Curve { command } => match command {
            CurveCommand::Show => {
                let info = legion::detect(&fs);
                let curve = info
                    .fan_curve
                    .ok_or_else(|| anyhow::anyhow!("本机未暴露 LenovoLegionLinux 风扇曲线"))?;
                if json {
                    print_json(&curve)?;
                } else {
                    println!(
                        "{}",
                        toml::to_string_pretty(&curve).context("cannot render curve")?
                    );
                }
            }
            CurveCommand::Save { file } => {
                let info = legion::detect(&fs);
                let curve = info
                    .fan_curve
                    .ok_or_else(|| anyhow::anyhow!("本机未暴露 LenovoLegionLinux 风扇曲线"))?;
                legion::save_curve(&curve, &file)?;
                println!("saved fan curve to {}", file.display());
            }
            CurveCommand::Apply { file } => {
                let curve = legion::load_curve(&file)?;
                report(
                    execute(ApplyRequest {
                        legion: Some(LegionRequest {
                            fan_curve: Some(curve),
                            ..Default::default()
                        }),
                        dry_run,
                        ..Default::default()
                    })?,
                    json,
                )?;
            }
        },
    }

    Ok(())
}

fn handle_tlp(command: TlpCommand, json: bool, dry_run: bool) -> Result<()> {
    match command {
        TlpCommand::Import { write } => {
            let fs = Sysfs::new();
            let imported = tlp_import::import(&fs);
            if json {
                print_json(&imported)?;
            } else {
                println!(
                    "{} TLP profiles: {}",
                    imported.profiles.profile.len(),
                    imported
                        .profiles
                        .profile
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                for note in &imported.notes {
                    println!("  note: {note}");
                }
                println!(
                    "AC → {} | battery → {} | auto_switch {}",
                    imported.profiles.power.ac,
                    imported.profiles.power.battery,
                    imported.profiles.power.auto_switch
                );
            }

            if write {
                if dry_run {
                    println!("dry-run: would write {}", profile::system_path().display());
                } else if privilege::is_root() {
                    imported.profiles.save(&profile::system_path())?;
                    println!("wrote {}", profile::system_path().display());
                } else {
                    bail!(
                        "writing {} requires root; run with sudo",
                        profile::system_path().display()
                    );
                }
            }
        }
    }
    Ok(())
}

fn takeover_report(import: bool, force: bool, dry_run: bool) -> Result<takeover::Report> {
    if privilege::is_root() || dry_run {
        return Ok(takeover::takeover(&Sysfs::new(), dry_run, import, force)?);
    }

    let mut args = vec!["--takeover"];
    if !import {
        args.push("--no-import");
    }
    if force {
        args.push("--force");
    }
    let stdout = privilege::run_helper_mode(&args)?;
    serde_json::from_str(&stdout).context("cannot parse helper report")
}

fn restore_report(dry_run: bool) -> Result<takeover::Report> {
    if privilege::is_root() || dry_run {
        return Ok(takeover::restore_tlp(dry_run)?);
    }
    let stdout = privilege::run_helper_mode(&["--restore-tlp"])?;
    serde_json::from_str(&stdout).context("cannot parse helper report")
}

fn print_takeover_report(report: &takeover::Report, json: bool) -> Result<()> {
    if json {
        print_json(report)?;
        return Ok(());
    }

    for step in &report.steps {
        let mark = if step.ok { "✓" } else { "×" };
        if step.detail.is_empty() {
            println!("{mark} {}", step.action);
        } else {
            println!("{mark} {} — {}", step.action, step.detail);
        }
    }
    for note in &report.notes {
        println!("  note: {note}");
    }
    for error in &report.errors {
        eprintln!("  error: {error}");
    }

    if report.dry_run {
        println!("预览完成，未做任何更改。");
    } else if report.ok() {
        println!("完成。");
    }
    Ok(())
}

fn handle_takeover(import: bool, force: bool, json: bool, dry_run: bool) -> Result<()> {
    let report = takeover_report(import, force, dry_run)?;
    print_takeover_report(&report, json)?;
    if !report.ok() {
        bail!("takeover finished with errors");
    }
    Ok(())
}

fn handle_restore_tlp(json: bool, dry_run: bool) -> Result<()> {
    let report = restore_report(dry_run)?;
    print_takeover_report(&report, json)?;
    if !report.ok() {
        bail!("restore finished with errors");
    }
    Ok(())
}

fn report(response: ApplyResponse, json: bool) -> Result<()> {
    if json {
        print_json(&response)?;
        return Ok(());
    }

    if response.dry_run {
        println!("Dry run — nothing was written.");
    }
    for write in &response.writes {
        println!(
            "{}: {} -> {}{}",
            write.target,
            write.previous.as_deref().unwrap_or("?"),
            write.value,
            if write.dry_run { " (preview)" } else { "" },
        );
    }
    for skipped in &response.skipped {
        println!("skipped: {skipped}");
    }
    for error in &response.errors {
        eprintln!("error: {error}");
    }

    if response.ok {
        Ok(())
    } else {
        bail!("{} operation(s) failed", response.errors.len())
    }
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn print_status(capabilities: &Capabilities) {
    println!(
        "Machine : {} {} ({}) BIOS {}",
        capabilities.dmi.sys_vendor.as_deref().unwrap_or("?"),
        capabilities.dmi.product_name.as_deref().unwrap_or("?"),
        capabilities.dmi.board_name.as_deref().unwrap_or("?"),
        capabilities.dmi.bios_version.as_deref().unwrap_or("?"),
    );
    println!(
        "Platform: {}",
        capabilities
            .platform
            .current
            .as_deref()
            .unwrap_or("unknown")
    );
    println!(
        "Power   : {}",
        match capabilities.power_supply.ac_online {
            Some(true) => "AC",
            Some(false) => "battery",
            None => "unknown",
        }
    );
    if let Some(battery) = capabilities.power_supply.batteries.first() {
        println!(
            "Battery : {}% {} | charge {}",
            battery
                .capacity
                .map(|value| value.to_string())
                .unwrap_or_else(|| "?".to_string()),
            battery.status.as_deref().unwrap_or("?"),
            battery.charge_type.as_deref().unwrap_or("?"),
        );
    }
    println!(
        "CPU     : {} cores | governor {} | EPP {} | {}..{} kHz",
        capabilities.cpu.cpus,
        capabilities.cpu.governor.as_deref().unwrap_or("?"),
        capabilities.cpu.epp.as_deref().unwrap_or("?"),
        capabilities.cpu.scaling_min_khz.unwrap_or(0),
        capabilities.cpu.scaling_max_khz.unwrap_or(0),
    );
    if let Some(gpu) = &capabilities.gpu {
        println!(
            "GPU     : {} | {:.0}°C | {:.1} W",
            gpu.name,
            gpu.temperature_c.unwrap_or(0.0),
            gpu.power_draw_w.unwrap_or(0.0),
        );
    }
    println!(
        "TLP     : {} (active {:?})",
        capabilities
            .tlp
            .version
            .as_deref()
            .unwrap_or("not installed"),
        capabilities.tlp.service_active
    );
    println!(
        "EC      : {} writable power attributes",
        capabilities.ec.attributes.len()
    );
}

fn print_probe(capabilities: &Capabilities) {
    print_status(capabilities);

    println!();
    println!("EC attributes");
    for attribute in &capabilities.ec.attributes {
        println!(
            "  {:<20} {:>6} {:<3} [{:?}..{:?}] default {:?} → {}",
            attribute.display_name,
            attribute
                .current
                .map(|value| value.to_string())
                .unwrap_or_else(|| "?".to_string()),
            attribute.unit,
            attribute.min,
            attribute.max,
            attribute.default,
            attribute.path,
        );
    }

    println!();
    println!(
        "Platform choices: {}",
        capabilities.platform.choices.join(" ")
    );
    println!(
        "Ideapad: {}",
        capabilities
            .ideapad
            .attributes
            .iter()
            .map(|attribute| format!(
                "{}={}",
                attribute.name,
                attribute.value.as_deref().unwrap_or("?")
            ))
            .collect::<Vec<_>>()
            .join(" ")
    );

    println!();
    println!("Sensors");
    for device in &capabilities.hwmon {
        if device.temperatures.is_empty() && device.fans.is_empty() && device.powers.is_empty() {
            continue;
        }
        let temps: Vec<String> = device
            .temperatures
            .iter()
            .map(|sensor| format!("{}={:.1}°C", sensor.label, sensor.value))
            .collect();
        let fans: Vec<String> = device
            .fans
            .iter()
            .map(|sensor| format!("{}={:.0}RPM", sensor.label, sensor.value))
            .collect();
        let powers: Vec<String> = device
            .powers
            .iter()
            .map(|sensor| format!("{}={:.1}W", sensor.label, sensor.value))
            .collect();
        println!(
            "  {:<16} {} {} {}",
            device.name,
            temps.join(" "),
            fans.join(" "),
            powers.join(" ")
        );
    }

    println!();
    println!(
        "Services: tlp {:?} | tlp-pd {:?} | nvidia-powerd {:?} | power-profiles-daemon {:?}",
        capabilities.services.tlp,
        capabilities.services.tlp_pd,
        capabilities.services.nvidia_powerd,
        capabilities.services.power_profiles_daemon,
    );
    println!(
        "Legion  : {}",
        if capabilities.legion.installed {
            "detected"
        } else {
            "not installed"
        }
    );
}
