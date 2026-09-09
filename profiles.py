"""Profile management for power settings."""

import json
import os

PROFILES_DIR = os.path.expanduser("~/.config/loq-power")
PROFILES_FILE = os.path.join(PROFILES_DIR, "profiles.json")

DEFAULT_PROFILES = {
    "省电": {
        "gpu_nv_ctgp": 55,
        "gpu_nv_ppab": 10,
        "gpu_nv_ac_offset": 10,
        "gpu_nv_cpu_boost": 5,
        "gpu_temp": 75,
        "ppt_pl1_spl": 40,
        "ppt_pl2_sppt": 45,
        "ppt_pl3_fppt": 55,
        "ppt_cpu_cl": 30,
        "cpu_temp": 85,
        "cpu_scaling_max_freq": 2800000,
        "cpu_governor": "powersave",
        "cpu_epp": "power",
        "platform_profile": "low-power",
    },
    "平衡": {
        "gpu_nv_ctgp": 60,
        "gpu_nv_ppab": 15,
        "gpu_nv_ac_offset": 45,
        "gpu_nv_cpu_boost": 5,
        "gpu_temp": 85,
        "ppt_pl1_spl": 45,
        "ppt_pl2_sppt": 50,
        "ppt_pl3_fppt": 55,
        "ppt_cpu_cl": 45,
        "cpu_temp": 95,
        "cpu_scaling_max_freq": 4000000,
        "cpu_governor": "performance",
        "cpu_epp": "performance",
        "platform_profile": "balanced",
    },
    "性能": {
        "gpu_nv_ctgp": 80,
        "gpu_nv_ppab": 25,
        "gpu_nv_ac_offset": 55,
        "gpu_nv_cpu_boost": 10,
        "gpu_temp": 87,
        "ppt_pl1_spl": 54,
        "ppt_pl2_sppt": 65,
        "ppt_pl3_fppt": 80,
        "ppt_cpu_cl": 45,
        "cpu_temp": 100,
        "cpu_scaling_max_freq": 4553000,
        "cpu_governor": "performance",
        "cpu_epp": "performance",
        "platform_profile": "performance",
    },
}


def _ensure_dir():
    os.makedirs(PROFILES_DIR, exist_ok=True)


def load_profiles():
    """Load profiles, merging defaults with user-saved."""
    profiles = dict(DEFAULT_PROFILES)
    if os.path.isfile(PROFILES_FILE):
        try:
            with open(PROFILES_FILE) as f:
                user = json.load(f)
            if isinstance(user, dict):
                profiles.update(user)
        except (json.JSONDecodeError, OSError):
            pass
    return profiles


def save_profiles(profiles):
    """Save user profiles to disk."""
    _ensure_dir()
    with open(PROFILES_FILE, "w") as f:
        json.dump(profiles, f, indent=2, ensure_ascii=False)


def save_user_profile(name, settings):
    """Save a single user profile."""
    profiles = load_profiles()
    profiles[name] = settings
    save_profiles(profiles)


def delete_user_profile(name):
    """Delete a user profile (cannot delete built-in ones)."""
    if name in DEFAULT_PROFILES:
        return False, "Cannot delete built-in profile"
    profiles = load_profiles()
    if name in profiles:
        del profiles[name]
        save_profiles(profiles)
        return True, ""
    return False, "Profile not found"


def get_builtin_names():
    return list(DEFAULT_PROFILES.keys())
