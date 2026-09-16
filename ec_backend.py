"""Backend for reading/writing EC firmware settings via sysfs."""

import os
import subprocess
import glob

FIRMWARE_ATTRS_BASE = "/sys/class/firmware-attributes"
CPUFREQ_BASE = "/sys/devices/system/cpu"
HELPER_SCRIPT = "/usr/local/bin/loq-power-apply"

# Cached sysfs paths (avoid re-scanning on every 3s dashboard tick)
_ATTRS_PATH_CACHE = None
_GAMEZONE_PROFILE_CACHE = None
_GAMEZONE_CHOICES_CACHE = None

# Service-status cache (systemctl is slow ~50-200ms each)
_SERVICE_CACHE = {}
_SERVICE_CACHE_TS = {}
_SERVICE_CACHE_TTL = 30  # seconds

# Custom 模式的风扇基底：按用户决策固定为 balanced
# (performance/max-power 在 Linux 下风扇直接拉满 5k RPM，无转速控制)
CUSTOM_FAN_BASIS = "balanced"


def _find_lenovo_attrs_path():
    """Find the lenovo-wmi-other attributes path (cached)."""
    global _ATTRS_PATH_CACHE
    if _ATTRS_PATH_CACHE is not None:
        return _ATTRS_PATH_CACHE
    if not os.path.isdir(FIRMWARE_ATTRS_BASE):
        return None
    for name in os.listdir(FIRMWARE_ATTRS_BASE):
        if "lenovo-wmi-other" in name:
            path = os.path.join(FIRMWARE_ATTRS_BASE, name, "attributes")
            if os.path.isdir(path):
                _ATTRS_PATH_CACHE = path
                return path
    return None


def _find_gamezone_profile_path():
    """Find the gamezone platform-profile path (cached).

    On LOQ models, the gamezone WMI driver exposes the thermal mode
    through a platform-profile device. This is the ONLY way to switch
    to CUSTOM mode programmatically (Fn+Q doesn't reach CUSTOM on LOQ).
    """
    global _GAMEZONE_PROFILE_CACHE
    if _GAMEZONE_PROFILE_CACHE is not None:
        return _GAMEZONE_PROFILE_CACHE
    pattern = "/sys/bus/wmi/drivers/lenovo_wmi_gamezone/*/platform-profile/platform-profile-0/profile"
    matches = glob.glob(pattern)
    if matches:
        _GAMEZONE_PROFILE_CACHE = matches[0]
        return matches[0]
    return None


def _find_gamezone_choices_path():
    global _GAMEZONE_CHOICES_CACHE
    if _GAMEZONE_CHOICES_CACHE is not None:
        return _GAMEZONE_CHOICES_CACHE
    profile = _find_gamezone_profile_path()
    if profile:
        choices = os.path.join(os.path.dirname(profile), "choices")
        if os.path.isfile(choices):
            _GAMEZONE_CHOICES_CACHE = choices
            return choices
    return None


def _read_file(path):
    try:
        with open(path) as f:
            return f.read().strip()
    except (PermissionError, OSError):
        return None


def _write_file_sudo(path, value):
    """Write a value to a sysfs file using sudo."""
    try:
        result = subprocess.run(
            ["sudo", "tee", path],
            input=str(value).encode(),
            capture_output=True,
            timeout=5,
        )
        return result.returncode == 0, result.stderr.decode().strip()
    except Exception as e:
        return False, str(e)


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


# ── Thermal mode ────────────────────────────────────────────────

def read_platform_profile():
    """Read current thermal mode from gamezone driver."""
    path = _find_gamezone_profile_path()
    if path:
        return _read_file(path)
    return None


def read_platform_profile_choices():
    path = _find_gamezone_choices_path()
    if path:
        return _read_file(path).split()
    return []


def is_custom_mode():
    """Check if the system is in CUSTOM thermal mode."""
    return read_platform_profile() == "custom"


def set_platform_profile(mode):
    """Switch thermal mode via gamezone driver. Requires sudo."""
    path = _find_gamezone_profile_path()
    if not path:
        return False, "Gamezone platform-profile not found"
    return _write_file_sudo(path, mode)


def ensure_custom_mode():
    """Ensure CUSTOM mode with balanced fan basis.

    软件切 custom 不改变 EC 风扇策略（风扇停留在上次 Fn+Q）。
    为保证 custom 下风扇可控，先切到 balanced（风扇正常温控），
    再切 custom（仅打开 EC 写权限）。performance/max-power 在
    Linux 下风扇直接拉满，不适合做 custom 基底。

    Returns:
        (bool, str) - (was_already_custom, error_message)
    """
    if is_custom_mode():
        return True, ""
    # Step 1: fan basis -> balanced (so custom inherits sane fan curve)
    ok, err = set_platform_profile(CUSTOM_FAN_BASIS)
    if not ok:
        return False, f"Failed to set fan basis ({CUSTOM_FAN_BASIS}): {err}"
    # Step 2: balanced -> custom (unlock EC writes, fan stays)
    ok, err = set_platform_profile("custom")
    if not ok:
        return False, f"Failed to switch to CUSTOM mode: {err}"
    # Verify
    if not is_custom_mode():
        return False, "Switched but CUSTOM mode not confirmed"
    return False, ""


def get_fan_basis():
    """Return effective fan basis.

    custom 本身无独立风扇曲线，返回固定基底 balanced；
    其他模式返回自身。
    """
    mode = read_platform_profile()
    if mode == "custom":
        return CUSTOM_FAN_BASIS
    return mode


def read_thermal_mode_summary():
    """Return a human-readable summary of the current thermal mode."""
    pp = read_platform_profile()
    if pp == "custom":
        return "custom", "CUSTOM - 可写入 EC 功率限制"
    mode_desc = {
        "low-power": "静音 - EC 功率限制只读",
        "balanced": "均衡 - EC 功率限制只读",
        "performance": "性能 - EC 功率限制只读",
        "max-power": "极速 - EC 功率限制只读",
    }
    desc = mode_desc.get(pp, f"{pp} - EC 功率限制只读")
    return pp, desc


# ── EC firmware attributes ──────────────────────────────────────

EC_ATTRIBUTES = {
    "gpu_nv_ctgp": {"name": "GPU cTGP", "desc": "可配置总图形功率", "unit": "W"},
    "gpu_nv_ppab": {"name": "GPU PPAB", "desc": "功率加速", "unit": "W"},
    "gpu_nv_ac_offset": {"name": "GPU AC Offset", "desc": "适配器功率偏移(仅插电有效)", "unit": "W"},
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
    """Apply settings in a single sudo call.

    Custom 是独立模式：target 由 UI 的平台下拉决定。
      - target == "custom" (或 None): 切 custom（均衡风扇基底）-> 写 EC -> 停住
      - target == 其他: 切 custom -> 写 EC -> 切回 target（EC值持久保留）

    EC 值做 diff：只写与当前值不同的项，减少 EBUSY/磨损。

    Returns:
        (bool, str) - (success, error_message)
    """
    if not os.path.isfile(HELPER_SCRIPT):
        return False, (
            f"Helper script not found: {HELPER_SCRIPT}\n"
            "Run: sudo cp loq-power-apply /usr/local/bin/ && sudo chmod +x /usr/local/bin/loq-power-apply"
        )

    target = platform_profile or "custom"

    # Diff EC values against hardware: skip unchanged
    filtered_ec = {}
    if ec_values:
        for attr_name, value in ec_values.items():
            if attr_name not in EC_ATTRIBUTES:
                continue
            try:
                cur = read_ec_value(attr_name)
            except Exception:
                cur = None
            if cur is None or int(cur) != int(value):
                filtered_ec[attr_name] = value

    # If EC values need writing, ensure CUSTOM mode first (balanced fan basis)
    if filtered_ec:
        already_custom, err = ensure_custom_mode()
        if err:
            return False, err

    # Build key=value input for the helper script.
    # NOTE: platform_profile must be LAST (helper enforces EC-first order).
    lines = []
    for attr_name, value in filtered_ec.items():
        lines.append(f"ec_{attr_name}={value}")
    if cpu_freq_mhz is not None:
        lines.append(f"cpu_max_freq={cpu_freq_mhz * 1000}")
    if governor:
        lines.append(f"cpu_governor={governor}")
    if epp:
        lines.append(f"cpu_epp={epp}")
    # Target mode always sent last so helper writes it after EC values.
    # "custom" target = stay in custom (fan basis already balanced).
    lines.append(f"platform_profile={target}")

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


# ── GPU full info (nvidia-smi) ──────────────────────────────────

def read_gpu_full():
    """Read comprehensive GPU info from nvidia-smi."""
    try:
        result = subprocess.run(
            ["nvidia-smi",
             "--query-gpu=name,temperature.gpu,utilization.gpu,utilization.memory,"
             "memory.used,memory.total,power.draw",
             "--format=csv,noheader,nounits"],
            capture_output=True, text=True, timeout=5,
        )
        if result.returncode != 0:
            return None
        parts = result.stdout.strip().split(", ")
        if len(parts) < 7:
            return None
        return {
            "name": parts[0],
            "temp": int(parts[1]),
            "gpu_util": int(parts[2]),
            "mem_util": int(parts[3]),
            "mem_used": int(parts[4]),
            "mem_total": int(parts[5]),
            "power_draw": float(parts[6]),
        }
    except Exception:
        return None


# ── CPU temperature ─────────────────────────────────────────────

def _read_hwmon_cpu_temp():
    """Prefer k10temp/zenpower (Tctl/Tdie), the real CPU sensor.

    thermal_zone0 on this LOQ is acpitz (motherboard), not CPU.
    """
    try:
        for hwmon_dir in sorted(os.listdir("/sys/class/hwmon")):
            hwmon_path = os.path.join("/sys/class/hwmon", hwmon_dir)
            name_file = os.path.join(hwmon_path, "name")
            if not os.path.isfile(name_file):
                continue
            name = _read_file(name_file)
            if name not in ("k10temp", "zenpower", "k8temp"):
                continue
            best = None
            for fname in sorted(os.listdir(hwmon_path)):
                if not (fname.startswith("temp") and fname.endswith("_input")):
                    continue
                prefix = fname[:-len("_input")]
                label = _read_file(os.path.join(hwmon_path, prefix + "_label"))
                # Prefer Tctl/Tdie/Tccd over composite readings
                val_raw = _read_file(os.path.join(hwmon_path, fname))
                if not val_raw:
                    continue
                try:
                    temp_c = int(val_raw) // 1000
                except ValueError:
                    continue
                if label in ("Tctl", "Tdie"):
                    return name, temp_c
                if best is None:
                    best = (name, temp_c)
            if best is not None:
                return best
    except OSError:
        pass
    return None, None


def read_cpu_temp():
    """Read CPU temperature: k10temp first, thermal zones fallback (skip acpitz)."""
    name, temp = _read_hwmon_cpu_temp()
    if temp is not None:
        return name, temp
    try:
        fallback = None
        for zone_dir in sorted(os.listdir("/sys/class/thermal")):
            if not zone_dir.startswith("thermal_zone"):
                continue
            zone_path = os.path.join("/sys/class/thermal", zone_dir)
            zone_type = _read_file(os.path.join(zone_path, "type"))
            temp_raw = _read_file(os.path.join(zone_path, "temp"))
            if not (zone_type and temp_raw):
                continue
            try:
                temp_c = int(temp_raw) // 1000
            except ValueError:
                continue
            if zone_type == "acpitz":
                # Motherboard sensor: keep only as last resort
                if fallback is None:
                    fallback = (zone_type, temp_c)
                continue
            return zone_type, temp_c
        if fallback is not None:
            return fallback
    except OSError:
        pass
    return None, None


# ── Battery info ────────────────────────────────────────────────

def read_battery():
    """Read battery info from sysfs."""
    result = {}
    for ps_dir in os.listdir("/sys/class/power_supply"):
        if not ps_dir.startswith("BAT"):
            continue
        ps_path = os.path.join("/sys/class/power_supply", ps_dir)
        capacity = _read_file(os.path.join(ps_path, "capacity"))
        status = _read_file(os.path.join(ps_path, "status"))
        voltage = _read_file(os.path.join(ps_path, "voltage_now"))
        energy = _read_file(os.path.join(ps_path, "energy_now"))
        if capacity:
            try:
                result["capacity"] = int(capacity)
            except ValueError:
                pass
        if status:
            result["status"] = status
        if voltage:
            try:
                result["voltage"] = int(voltage) // 1000000
            except ValueError:
                pass
        if energy:
            try:
                result["energy_wh"] = int(energy) // 1000
            except ValueError:
                pass
        break
    return result


# ── AC adapter ──────────────────────────────────────────────────

def read_ac_status():
    """Read AC adapter status."""
    for ps_dir in os.listdir("/sys/class/power_supply"):
        ps_path = os.path.join("/sys/class/power_supply", ps_dir)
        ps_type = _read_file(os.path.join(ps_path, "type"))
        if ps_type == "Mains":
            online = _read_file(os.path.join(ps_path, "online"))
            return online == "1"
    return None


# ── Service status ──────────────────────────────────────────────

def read_service_status(service_name):
    """Check if a systemd service is active (cached 30s; systemctl is slow)."""
    import time
    now = time.monotonic()
    ts = _SERVICE_CACHE_TS.get(service_name)
    if ts is not None and (now - ts) < _SERVICE_CACHE_TTL and service_name in _SERVICE_CACHE:
        return _SERVICE_CACHE[service_name]
    try:
        result = subprocess.run(
            ["systemctl", "is-active", service_name],
            capture_output=True, text=True, timeout=3,
        )
        active = result.stdout.strip() == "active"
        _SERVICE_CACHE[service_name] = active
        _SERVICE_CACHE_TS[service_name] = now
        return active
    except Exception:
        return None


# ── System dashboard data ───────────────────────────────────────

def read_battery_charge_type():
    """Read current battery charge type (Fast/Standard/Long_Life)."""
    for ps_dir in os.listdir("/sys/class/power_supply"):
        if not ps_dir.startswith("BAT"):
            continue
        ps_path = os.path.join("/sys/class/power_supply", ps_dir)
        ct = _read_file(os.path.join(ps_path, "charge_types"))
        if ct:
            # Format: "Fast Standard [Long_Life]" - brackets show current
            current = None
            available = []
            for token in ct.split():
                if token.startswith("[") and token.endswith("]"):
                    current = token.strip("[]")
                    available.append(current)
                else:
                    available.append(token)
            return current, available
    return None, []


def set_battery_charge_type(charge_type):
    """Set battery charge type. Requires sudo."""
    for ps_dir in os.listdir("/sys/class/power_supply"):
        if not ps_dir.startswith("BAT"):
            continue
        ps_path = os.path.join("/sys/class/power_supply", ps_dir)
        ct_path = os.path.join(ps_path, "charge_types")
        if os.path.isfile(ct_path):
            return _write_file_sudo(ct_path, charge_type)
    return False, "Battery charge_types not found"


def read_dashboard():
    """Read all dashboard data in one call for efficiency."""
    gpu = read_gpu_full()
    cpu_temp_name, cpu_temp = read_cpu_temp()
    battery = read_battery()
    ac = read_ac_status()
    fan = read_fan_info()
    cpu = read_cpu_freq()
    ec = read_all_ec()
    tlp = read_service_status("tlp")
    nv_powerd = read_service_status("nvidia-powerd")
    mode = read_platform_profile()
    fan_basis = CUSTOM_FAN_BASIS if mode == "custom" else mode
    charge_type, charge_choices = read_battery_charge_type()

    return {
        "gpu": gpu,
        "cpu_temp_name": cpu_temp_name,
        "cpu_temp": cpu_temp,
        "battery": battery,
        "ac_power": ac,
        "fan": fan,
        "cpu": cpu,
        "ec": ec,
        "tlp": tlp,
        "nv_powerd": nv_powerd,
        "mode": mode,
        "fan_basis": fan_basis,
        "charge_type": charge_type,
        "charge_choices": charge_choices,
    }
