use anyhow::Result;
use clap::{Parser, Subcommand};
use loq_core::Capabilities;

#[derive(Debug, Parser)]
#[command(
    name = "loq-power",
    version,
    about = "LOQ Power — EC and power management for Lenovo LOQ laptops"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Discover every EC/power capability exposed by the kernel
    Probe {
        /// Print the full capability manifest as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show a short status summary
    Status {
        /// Print the full capability manifest as JSON
        #[arg(long)]
        json: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Probe { json } | Command::Status { json } => {
            let capabilities = loq_core::probe();
            if json {
                println!("{}", serde_json::to_string_pretty(&capabilities)?);
            } else {
                print_human(&capabilities);
            }
        }
    }
    Ok(())
}

fn print_human(capabilities: &Capabilities) {
    println!("LOQ Power probe");
    println!(
        "Host    : {}",
        capabilities.hostname.as_deref().unwrap_or("unknown")
    );
    println!(
        "Kernel  : {}",
        capabilities.kernel.as_deref().unwrap_or("unknown")
    );
    println!(
        "Machine : {} {} ({}) BIOS {}",
        capabilities.dmi.sys_vendor.as_deref().unwrap_or("?"),
        capabilities.dmi.product_name.as_deref().unwrap_or("?"),
        capabilities.dmi.board_name.as_deref().unwrap_or("?"),
        capabilities.dmi.bios_version.as_deref().unwrap_or("?"),
    );

    println!();
    println!(
        "EC / firmware attributes ({})",
        capabilities.ec.attributes.len()
    );
    if let Some(base) = &capabilities.ec.base {
        println!("  base: {base}");
    }
    for attribute in &capabilities.ec.attributes {
        let range = match (attribute.min, attribute.max) {
            (Some(min), Some(max)) => format!("[{min}..{max}]"),
            _ => "[?]".to_string(),
        };
        let default = attribute
            .default
            .map(|value| format!(" default {value}"))
            .unwrap_or_default();
        let step = attribute
            .step
            .map(|value| format!(" step {value}"))
            .unwrap_or_default();
        println!(
            "  {:<20} {:>6} {:<3} {:<14}{}{}",
            attribute.display_name,
            attribute
                .current
                .map(|value| value.to_string())
                .unwrap_or_else(|| "?".to_string()),
            attribute.unit,
            range,
            default,
            step,
        );
    }

    println!();
    println!(
        "Platform: {} | choices: {}",
        capabilities.platform.current.as_deref().unwrap_or("N/A"),
        capabilities.platform.choices.join(" ")
    );
    println!(
        "ACPI    : {} | choices: {}",
        capabilities
            .platform
            .acpi_current
            .as_deref()
            .unwrap_or("N/A"),
        capabilities.platform.acpi_choices.join(" ")
    );

    println!();
    for battery in &capabilities.power_supply.batteries {
        println!(
            "Battery {}: {}% {} | charge {:?} {:?}",
            battery.name,
            battery
                .capacity
                .map(|value| value.to_string())
                .unwrap_or_else(|| "?".to_string()),
            battery.status.as_deref().unwrap_or("?"),
            battery.charge_type,
            battery.charge_types,
        );
    }
    println!(
        "AC      : {}",
        match capabilities.power_supply.ac_online {
            Some(true) => "online",
            Some(false) => "battery",
            None => "unknown",
        }
    );

    println!();
    println!(
        "CPU     : {} cores | {} | governor {} | EPP {} | {}..{} kHz | boost {:?}",
        capabilities.cpu.cpus,
        capabilities.cpu.driver.as_deref().unwrap_or("?"),
        capabilities.cpu.governor.as_deref().unwrap_or("?"),
        capabilities.cpu.epp.as_deref().unwrap_or("?"),
        capabilities.cpu.scaling_min_khz.unwrap_or(0),
        capabilities.cpu.scaling_max_khz.unwrap_or(0),
        capabilities.cpu.boost,
    );
    println!(
        "          EPP choices: {}",
        capabilities.cpu.available_epp.join(" ")
    );

    println!();
    match &capabilities.gpu {
        Some(gpu) => println!(
            "GPU     : {} | {:.0}°C | {:.0}W | util {:.0}% | mem {:.0}/{:.0} MiB",
            gpu.name,
            gpu.temperature_c.unwrap_or(0.0),
            gpu.power_draw_w.unwrap_or(0.0),
            gpu.utilization_percent.unwrap_or(0.0),
            gpu.memory_used_mib.unwrap_or(0.0),
            gpu.memory_total_mib.unwrap_or(0.0),
        ),
        None => println!("GPU     : nvidia-smi not available"),
    }

    println!();
    println!(
        "TLP     : {:?} | service active {:?} enabled {:?} | tlp-pd active {:?}",
        capabilities.tlp.version,
        capabilities.tlp.service_active,
        capabilities.tlp.service_enabled,
        capabilities.tlp.pd_active,
    );
    for (key, value) in &capabilities.tlp.effective {
        if key.starts_with("CPU_")
            || key.starts_with("PLATFORM_PROFILE")
            || key.starts_with("STOP_CHARGE")
            || key.starts_with("PCIE_ASPM")
            || key.starts_with("WIFI_PWR")
        {
            println!("  {key}={value}");
        }
    }

    println!();
    println!(
        "Ideapad : {}",
        capabilities
            .ideapad
            .base
            .as_deref()
            .unwrap_or("not available")
    );
    for attribute in &capabilities.ideapad.attributes {
        println!(
            "  {:<20} {:>6}  {}{}",
            attribute.name,
            attribute.value.as_deref().unwrap_or("?"),
            if attribute.known { "" } else { "(unknown) " },
            if attribute.writable { "rw" } else { "ro" },
        );
    }

    println!();
    println!("LEDs    :");
    for led in &capabilities.leds {
        println!(
            "  {:<28} {}/{}",
            led.name,
            led.brightness
                .map(|value| value.to_string())
                .unwrap_or_else(|| "?".to_string()),
            led.max_brightness
                .map(|value| value.to_string())
                .unwrap_or_else(|| "?".to_string()),
        );
    }

    println!();
    println!("Sensors : {} hwmon device(s)", capabilities.hwmon.len());
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
        "Legion  : {}",
        if capabilities.legion.installed {
            "detected"
        } else {
            "not installed (fan curve unavailable)"
        }
    );
    if let Some(path) = &capabilities.legion.powermode_path {
        println!("  powermode: {:?} ({path})", capabilities.legion.powermode);
    }
    if let Some(path) = &capabilities.legion.fancurve_path {
        println!(
            "  fancurve: {} ({path})",
            if capabilities.legion.fancurve.is_some() {
                "readable"
            } else {
                "not readable (try root)"
            }
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
}
