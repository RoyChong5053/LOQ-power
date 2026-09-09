"""Backend for reading/writing EC firmware settings via sysfs."""

import os
import subprocess

FIRMWARE_ATTRS_BASE = "/sys/class/firmware-attributes"
CPUFREQ_BASE = "/sys/devices/system/cpu"
PLATFORM_PROFILE = "/sys/firmware/acpi/platform_profile"


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


def _read_attr(attr_name, field="current_value"):
    base = _find_lenovo_attrs_path()
    if not base:
        return None
    fpath = os.path.join(base, attr_name, field)
    if not os.path.isfile(fpath):
        return None
    try:
        with open(fpath) as f:
            return f.read().strip()
    except (PermissionError, OSError):
        return None


def _read_attr_meta(attr_name):
    base = _find_lenovo_attrs_path()
    if not base:
        return {}
    meta = {}
    for field in ("min_value", "max_value", "default_value", "scalar_increment", "display_name"):
        fpath = os.path.join(base, attr_name, field)
        if os.path.isfile(fpath):
            try:
                with open(fpath) as f:
                    meta[field] = f.read().strip()
            except (PermissionError, OSError):
                pass
    return meta


def _write_attr_root(attr_name, value):
    """Write to an EC attribute using pkexec for root privileges."""
    base = _find_lenovo_attrs_path()
    if not base:
        return False, "Firmware attributes not found"
    fpath = os.path.join(base, attr_name, "current_value")
    if not os.path.isfile(fpath):
        return False, f"Attribute {attr_name} not found"
    try:
        result = subprocess.run(
            ["pkexec", "tee", fpath],
            input=str(value).encode(),
            capture_output=True,
            timeout=10,
        )
        if result.returncode != 0:
            stderr = result.stderr.decode().strip()
            if "cancelled" in stderr.lower() or "dismissed" in stderr.lower():
                return False, "Cancelled by user"
            return False, stderr or "Write failed"
        return True, ""
    except subprocess.TimeoutExpired:
        return False, "Timeout"
    except Exception as e:
        return False, str(e)


# ── EC firmware attributes ──────────────────────────────────────

EC_ATTRIBUTES = {
    "gpu_nv_ctgp": {
        "name": "GPU cTGP",
        "desc": "Configurable Total Graphics Power",
        "unit": "W",
    },
    "gpu_nv_ppab": {
        "name": "GPU PPAB",
        "desc": "Power Performance Aware Boost",
        "unit": "W",
    },
    "gpu_nv_ac_offset": {
        "name": "GPU AC Offset",
        "desc": "Total Processing Power Baseline Offset",
        "unit": "W",
    },
    "gpu_nv_cpu_boost": {
        "name": "GPU→CPU Boost",
        "desc": "Dynamic Boost Limit",
        "unit": "W",
    },
    "gpu_temp": {
        "name": "GPU Temp Limit",
        "desc": "GPU Thermal Load Limit",
        "unit": "°C",
    },
    "ppt_pl1_spl": {
        "name": "CPU PL1",
        "desc": "Sustained Power Limit",
        "unit": "W",
    },
    "ppt_pl2_sppt": {
        "name": "CPU PL2",
        "desc": "Short Term Power Limit",
        "unit": "W",
    },
    "ppt_pl3_fppt": {
        "name": "CPU PL3",
        "desc": "Fast Package Power Tracking",
        "unit": "W",
    },
    "ppt_cpu_cl": {
        "name": "CPU Cross Load",
        "desc": "Cross Loading Power Limit",
        "unit": "W",
    },
    "cpu_temp": {
        "name": "CPU Temp Limit",
        "desc": "CPU Thermal Load Limit",
        "unit": "°C",
    },
}


def read_ec_value(attr_name):
    """Read current EC attribute value as int."""
    val = _read_attr(attr_name, "current_value")
    if val is None:
        return None
    try:
        return int(val)
    except ValueError:
        return None


def read_ec_meta(attr_name):
    """Read EC attribute metadata (min, max, default)."""
    meta = _read_attr_meta(attr_name)
    result = {}
    for key in ("min_value", "max_value", "default_value"):
        val = meta.get(key)
        if val is not None:
            try:
                result[key] = int(val)
            except ValueError:
                result[key] = val
        else:
            result[key] = None
    return result


def write_ec_value(attr_name, value):
    """Write value to EC attribute (requires root)."""
    return _write_attr_root(attr_name, value)


def read_all_ec():
    """Read all EC attribute values and metadata."""
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


# ── CPU frequency scaling ───────────────────────────────────────

def _get_cpu_count():
    count = 0
    for name in os.listdir(CPUFREQ_BASE):
        if name.startswith("cpu") and name[3:].isdigit():
            count += 1
    return count


def read_cpu_freq():
    """Read CPU frequency settings."""
    n = _get_cpu_count()
    if n == 0:
        return {}
    cpu0 = os.path.join(CPUFREQ_BASE, "cpu0", "cpufreq")
    result = {}
    for field, fname in [
        ("scaling_max_freq", "scaling_max_freq"),
        ("scaling_min_freq", "scaling_min_freq"),
        ("scaling_governor", "scaling_governor"),
        ("energy_performance_preference", "energy_performance_preference"),
        ("cpuinfo_max_freq", "cpuinfo_max_freq"),
        ("cpuinfo_min_freq", "cpuinfo_min_freq"),
    ]:
        fpath = os.path.join(cpu0, fname)
        if os.path.isfile(fpath):
            try:
                with open(fpath) as f:
                    result[field] = f.read().strip()
            except (PermissionError, OSError):
                pass
    result["cpu_count"] = n
    return result


def _write_cpu_sysfs(field, value, per_cpu=False):
    """Write to CPU sysfs files. Requires root."""
    n = _get_cpu_count()
    if n == 0:
        return False, "No CPUs found"
    if per_cpu:
        paths = [
            os.path.join(CPUFREQ_BASE, f"cpu{i}", "cpufreq", field)
            for i in range(n)
        ]
    else:
        paths = [os.path.join(CPUFREQ_BASE, "cpu0", "cpufreq", field)]

    # Build a shell script that writes to all paths
    cmds = []
    for p in paths:
        if os.path.isfile(p):
            cmds.append(f'echo "{value}" > "{p}"')
    if not cmds:
        return False, "No valid CPU sysfs paths found"

    script = " && ".join(cmds)
    try:
        result = subprocess.run(
            ["pkexec", "bash", "-c", script],
            capture_output=True,
            timeout=10,
        )
        if result.returncode != 0:
            stderr = result.stderr.decode().strip()
            if "cancelled" in stderr.lower() or "dismissed" in stderr.lower():
                return False, "Cancelled by user"
            return False, stderr or "Write failed"
        return True, ""
    except subprocess.TimeoutExpired:
        return False, "Timeout"
    except Exception as e:
        return False, str(e)


def write_cpu_scaling_max_freq(freq_khz):
    return _write_cpu_sysfs("scaling_max_freq", freq_khz, per_cpu=True)


def write_cpu_scaling_min_freq(freq_khz):
    return _write_cpu_sysfs("scaling_min_freq", freq_khz, per_cpu=True)


def write_cpu_governor(governor):
    return _write_cpu_sysfs("scaling_governor", governor, per_cpu=True)


def write_cpu_epp(epp):
    return _write_cpu_sysfs("energy_performance_preference", epp, per_cpu=True)


# ── Platform profile ────────────────────────────────────────────

def read_platform_profile():
    if not os.path.isfile(PLATFORM_PROFILE):
        return None
    try:
        with open(PLATFORM_PROFILE) as f:
            return f.read().strip()
    except (PermissionError, OSError):
        return None


def read_platform_profile_choices():
    choices_path = PLATFORM_PROFILE + "_choices"
    if not os.path.isfile(choices_path):
        return []
    try:
        with open(choices_path) as f:
            return f.read().strip().split()
    except (PermissionError, OSError):
        return []


def write_platform_profile(profile):
    if not os.path.isfile(PLATFORM_PROFILE):
        return False, "Platform profile not found"
    try:
        result = subprocess.run(
            ["pkexec", "tee", PLATFORM_PROFILE],
            input=profile.encode(),
            capture_output=True,
            timeout=10,
        )
        if result.returncode != 0:
            stderr = result.stderr.decode().strip()
            if "cancelled" in stderr.lower() or "dismissed" in stderr.lower():
                return False, "Cancelled by user"
            return False, stderr or "Write failed"
        return True, ""
    except subprocess.TimeoutExpired:
        return False, "Timeout"
    except Exception as e:
        return False, str(e)


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
            elif "Average Power Draw" in line and "GPU" not in info:
                val = line.split(":")[-1].strip().replace(" W", "")
                try:
                    info["power_draw"] = float(val)
                except ValueError:
                    pass
        return info if info else None
    except Exception:
        return None
