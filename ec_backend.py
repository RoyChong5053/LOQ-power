"""Backend for reading/writing EC firmware settings via sysfs."""

import os
import subprocess

FIRMWARE_ATTRS_BASE = "/sys/class/firmware-attributes"
CPUFREQ_BASE = "/sys/devices/system/cpu"
PLATFORM_PROFILE = "/sys/firmware/acpi/platform_profile"
HELPER_SCRIPT = "/usr/local/bin/loq-power-apply"

# Thermal mode enum from kernel wmi-helpers.h
THERMAL_MODES = {
    0x00: "none",
    0x01: "quiet",
    0x02: "balanced",
    0x03: "performance",
    0xE0: "extreme",
    0xFF: "custom",
}


def _find_lenovo_attrs_path():
    """Find the lenovo-wmi-other attributes path."""
    if not os.path.isdir(FIRMWARE_ATTRS_BASE):
        return None
    for name in os.listdir(FIRMWARE_ATTRS_BASE):
        if "lenovo-wmi-other" in name:
            path = os.path.join(FIRMWARE_ATTRS_BASE, name, "attributes")
            if os.path.isdir(path):
                return path
    return None


def _read_file(path):
    try:
        with open(path) as f:
            return f.read().strip()
    except (PermissionError, OSError):
        return None


def _read_attr(attr_name, field="current_value"):
    base = _find_lenovo_attrs_path()
    if not base:
        return None
    fpath = os.path.join(base, attr_name, field)
    return _read_file(fpath) if os.path.isfile(fpath) else None


def _read_attr_meta(attr_name):
    base = _find_lenovo_attrs_path()
    if not base:
        return {}
    meta = {}
    for field in ("min_value", "max_value", "default_value", "scalar_increment"):
        fpath = os.path.join(base, attr_name, field)
        if os.path.isfile(fpath):
            val = _read_file(fpath)
            if val is not None:
                try:
                    meta[field] = int(val)
                except ValueError:
                    meta[field] = val
    return meta


# ── Thermal mode detection ─────────────────────────────────────

def read_platform_profile():
    """Read current platform profile (thermal mode indicator)."""
    return _read_file(PLATFORM_PROFILE)


def read_platform_profile_choices():
    choices_path = PLATFORM_PROFILE + "_choices"
    return _read_file(choices_path).split() if os.path.isfile(choices_path) else []


def is_custom_mode():
    """Check if the system is in CUSTOM thermal mode (0xFF).

    The kernel lenovo_wmi_other driver requires CUSTOM mode to write EC values.
    Fn+Q switches to CUSTOM (purple LED). platform_profile may not reflect this
    because the gamezone driver doesn't expose CUSTOM via platform_profile.
    We detect CUSTOM by attempting a test write.
    """
    pp = read_platform_profile()
    if pp == "custom":
        return True

    # platform_profile might not show "custom" even when in custom mode
    # because the gamezone driver doesn't map it properly.
    # Try a test write to detect custom mode.
    base = _find_lenovo_attrs_path()
    if not base:
        return False

    # Read a value, modify it slightly, write it back, then restore
    test_attr = "gpu_nv_cpu_boost"
    test_path = os.path.join(base, test_attr, "current_value")
    if not os.path.isfile(test_path):
        return False

    try:
        with open(test_path) as f:
            original = f.read().strip()
    except (PermissionError, OSError):
        return False

    # Try writing the same value back (no actual change)
    try:
        result = subprocess.run(
            ["sudo", "tee", test_path],
            input=original.encode(),
            capture_output=True,
            timeout=5,
        )
        if result.returncode == 0:
            return True
        stderr = result.stderr.decode().strip()
        if "busy" in stderr.lower():
            return False
        # Other errors might mean different things
        return False
    except Exception:
        return False


def read_thermal_mode_summary():
    """Return a human-readable summary of the current thermal mode."""
    pp = read_platform_profile()
    if pp == "custom":
        return "custom", "CUSTOM (Fn+Q 紫灯) - 可写入 EC"

    # Check if actually in custom mode despite platform_profile
    if is_custom_mode():
        return "custom", "CUSTOM (Fn+Q 紫灯) - 可写入 EC"

    mode_desc = {
        "low-power": "静音 (Fn+Q 白灯) - EC 只读",
        "balanced": "均衡 (Fn+Q 白灯) - EC 只读",
        "performance": "性能 (Fn+Q 红灯) - EC 只读",
        "max-power": "极速 (Fn+Q 红灯) - EC 只读",
    }
    desc = mode_desc.get(pp, f"{pp} - EC 只读")
    return pp, desc


# ── EC firmware attributes ──────────────────────────────────────

EC_ATTRIBUTES = {
    "gpu_nv_ctgp": {"name": "GPU cTGP", "desc": "可配置总图形功率", "unit": "W"},
    "gpu_nv_ppab": {"name": "GPU PPAB", "desc": "功率加速", "unit": "W"},
    "gpu_nv_ac_offset": {"name": "GPU AC Offset", "desc": "总功率偏移", "unit": "W"},
    "gpu_nv_cpu_boost": {"name": "GPU→CPU Boost", "desc": "动态加速", "unit": "W"},
    "gpu_temp": {"name": "GPU 温度限制", "desc": "GPU 热负载限制", "unit": "°C"},
    "ppt_pl1_spl": {"name": "CPU PL1", "desc": "持续功率限制", "unit": "W"},
    "ppt_pl2_sppt": {"name": "CPU PL2", "desc": "短时功率限制", "unit": "W"},
    "ppt_pl3_fppt": {"name": "CPU PL3", "desc": "快速功率跟踪", "unit": "W"},
    "ppt_cpu_cl": {"name": "CPU 交叉负载", "desc": "交叉负载功率", "unit": "W"},
    "cpu_temp": {"name": "CPU 温度限制", "desc": "CPU 热负载限制", "unit": "°C"},
}


def read_ec_value(attr_name):
    val = _read_attr(attr_name, "current_value")
    if val is None:
        return None
    try:
        return int(val)
    except ValueError:
        return None


def read_ec_meta(attr_name):
    meta = _read_attr_meta(attr_name)
    return {
        "min_value": meta.get("min_value"),
        "max_value": meta.get("max_value"),
        "default_value": meta.get("default_value"),
    }


def read_all_ec():
    result = {}
    for attr_name, info in EC_ATTRIBUTES.items():
        val = read_ec_value(attr_name)
        meta = read_ec_meta(attr_name)
        result[attr_name] = {
            **info,
            "value": val,
            "min": meta.get("min_value"),
            "max": meta.get("max_value"),
            "default": meta.get("default_value"),
        }
    return result


# ── Apply settings ─────────────────────────────────────────────

def apply_all_settings(ec_values, cpu_freq_mhz=None, governor=None, epp=None, platform_profile=None):
    """Apply ALL settings in a single sudo call.

    Returns:
        (bool, str) - (success, error_message)
    """
    # Check if helper script exists
    if not os.path.isfile(HELPER_SCRIPT):
        return False, (
            f"Helper script not found: {HELPER_SCRIPT}\n"
            "Run: sudo cp loq-power-apply /usr/local/bin/ && sudo chmod +x /usr/local/bin/loq-power-apply"
        )

    # Check if EC writes are possible (custom mode required)
    if ec_values:
        custom = is_custom_mode()
        if not custom:
            return False, (
                "EC 写入需要 CUSTOM 模式 (Fn+Q 紫灯)\n"
                "请先按 Fn+Q 切换到紫灯，再点击应用"
            )

    # Build key=value input for the helper script
    lines = []
    for attr_name, value in ec_values.items():
        lines.append(f"ec_{attr_name}={value}")
    if cpu_freq_mhz is not None:
        lines.append(f"cpu_max_freq={cpu_freq_mhz * 1000}")
    if governor:
        lines.append(f"cpu_governor={governor}")
    if epp:
        lines.append(f"cpu_epp={epp}")
    if platform_profile:
        lines.append(f"platform_profile={platform_profile}")

    if not lines:
        return False, "No settings to apply"

    stdin_data = "\n".join(lines) + "\n"

    try:
        result = subprocess.run(
            ["sudo", HELPER_SCRIPT],
            input=stdin_data.encode(),
            capture_output=True,
            timeout=15,
        )
        if result.returncode != 0:
            stderr = result.stderr.decode().strip()
            stdout = result.stdout.decode().strip()
            # Check for specific errors
            if "Device or resource busy" in stderr:
                return False, (
                    "EC 写入失败: 内核要求 CUSTOM 模式\n"
                    "请按 Fn+Q 切换到紫灯 (CUSTOM) 后重试"
                )
            return False, stderr or stdout or "Write failed"
        return True, ""
    except subprocess.TimeoutExpired:
        return False, "Timeout"
    except Exception as e:
        return False, str(e)


# ── CPU frequency scaling ───────────────────────────────────────

def _get_cpu_count():
    return sum(1 for name in os.listdir(CPUFREQ_BASE)
               if name.startswith("cpu") and name[3:].isdigit())


def read_cpu_freq():
    n = _get_cpu_count()
    if n == 0:
        return {}
    cpu0 = os.path.join(CPUFREQ_BASE, "cpu0", "cpufreq")
    result = {}
    for fname in ("scaling_max_freq", "scaling_min_freq", "scaling_governor",
                   "energy_performance_preference", "cpuinfo_max_freq", "cpuinfo_min_freq"):
        fpath = os.path.join(cpu0, fname)
        if os.path.isfile(fpath):
            val = _read_file(fpath)
            if val is not None:
                result[fname] = val
    result["cpu_count"] = n
    return result


# ── NVIDIA GPU info ─────────────────────────────────────────────

def read_nvidia_power():
    """Read GPU power info from nvidia-smi."""
    try:
        result = subprocess.run(
            ["nvidia-smi", "-q"],
            capture_output=True, text=True, timeout=5,
        )
        if result.returncode != 0:
            return None
        info = {}
        for line in result.stdout.splitlines():
            line = line.strip()
            if "Current Power Limit" in line:
                val = line.split(":")[-1].strip().replace(" W", "")
                try:
                    info["current_power_limit"] = float(val)
                except ValueError:
                    pass
            elif "Max Power Limit" in line:
                val = line.split(":")[-1].strip().replace(" W", "")
                try:
                    info["max_power_limit"] = float(val)
                except ValueError:
                    pass
            elif "Default Power Limit" in line:
                val = line.split(":")[-1].strip().replace(" W", "")
                try:
                    info["default_power_limit"] = float(val)
                except ValueError:
                    pass
            elif "Average Power Draw" in line and "power_draw" not in info:
                val = line.split(":")[-1].strip().replace(" W", "")
                try:
                    info["power_draw"] = float(val)
                except ValueError:
                    pass
        return info if info else None
    except Exception:
        return None


# ── Fan info ────────────────────────────────────────────────────

def read_fan_info():
    """Read fan speed from acpi_fan hwmon."""
    result = {}
    for hwmon_dir in sorted(os.listdir("/sys/class/hwmon")):
        hwmon_path = os.path.join("/sys/class/hwmon", hwmon_dir)
        name_file = os.path.join(hwmon_path, "name")
        if not os.path.isfile(name_file):
            continue
        name = _read_file(name_file)
        if name == "acpi_fan":
            for fan_file in sorted(os.listdir(hwmon_path)):
                if fan_file.startswith("fan") and fan_file.endswith("_input"):
                    fan_id = fan_file.replace("_input", "")
                    val = _read_file(os.path.join(hwmon_path, fan_file))
                    if val:
                        try:
                            result[fan_id] = int(val)
                        except ValueError:
                            pass
    return result
