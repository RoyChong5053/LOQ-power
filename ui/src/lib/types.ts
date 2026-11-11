export interface EcAttribute {
  name: string;
  display_name: string;
  description: string;
  unit: string;
  current: number | null;
  min: number | null;
  max: number | null;
  default: number | null;
  step: number | null;
  writable: boolean;
  write_requires_root: boolean;
  path: string;
}

export interface EcInfo {
  base: string | null;
  attributes: EcAttribute[];
}

export interface PlatformInfo {
  current: string | null;
  choices: string[];
  gamezone_profile: string | null;
  acpi_current: string | null;
  acpi_choices: string[];
}

export interface BatteryInfo {
  name: string;
  capacity: number | null;
  status: string | null;
  voltage_v: number | null;
  energy_now_wh: number | null;
  energy_full_wh: number | null;
  energy_full_design_wh: number | null;
  power_w: number | null;
  cycle_count: number | null;
  charge_type: string | null;
  charge_types: string[];
}

export interface PowerSupplyInfo {
  batteries: BatteryInfo[];
  ac_online: boolean | null;
  ac_name: string | null;
}

export interface CpuInfo {
  cpus: number;
  driver: string | null;
  governor: string | null;
  available_governors: string[];
  epp: string | null;
  available_epp: string[];
  scaling_min_khz: number | null;
  scaling_max_khz: number | null;
  scaling_cur_khz: number | null;
  cpuinfo_min_khz: number | null;
  cpuinfo_max_khz: number | null;
  boost: boolean | null;
  amd_pstate_status: string | null;
  dynamic_epp: string | null;
  prefcore: string | null;
}

export interface GpuInfo {
  name: string;
  temperature_c: number | null;
  utilization_percent: number | null;
  memory_utilization_percent: number | null;
  memory_used_mib: number | null;
  memory_total_mib: number | null;
  power_draw_w: number | null;
  power_limit_w: number | null;
  power_min_limit_w: number | null;
  power_max_limit_w: number | null;
  power_default_limit_w: number | null;
  clocks_graphics_mhz: number | null;
  clocks_max_graphics_mhz: number | null;
  clocks_memory_mhz: number | null;
  clocks_max_memory_mhz: number | null;
}

export interface Sensor {
  label: string;
  value: number;
  unit: string;
}

export interface HwmonDevice {
  name: string;
  path: string;
  temperatures: Sensor[];
  fans: Sensor[];
  powers: Sensor[];
}

export interface IdeapadAttribute {
  name: string;
  value: string | null;
  writable: boolean;
  write_requires_root: boolean;
  known: boolean;
  description: string;
  path: string;
}

export interface IdeapadInfo {
  base: string | null;
  attributes: IdeapadAttribute[];
}

export interface LedInfo {
  name: string;
  brightness: number | null;
  max_brightness: number | null;
  path: string;
}

export interface TlpInfo {
  installed: boolean;
  version: string | null;
  service_active: boolean | null;
  service_enabled: boolean | null;
  pd_active: boolean | null;
  pd_enabled: boolean | null;
  config_files: string[];
  effective: Record<string, string>;
}

export interface ServicesInfo {
  tlp: boolean | null;
  tlp_pd: boolean | null;
  nvidia_powerd: boolean | null;
  power_profiles_daemon: boolean | null;
}

export interface LegionInfo {
  installed: boolean;
  hwmon_path: string | null;
  fancurve_path: string | null;
  fancurve: string | null;
  powermode_path: string | null;
  powermode: string | null;
  temperatures: Sensor[];
  fans: Sensor[];
}

export interface Capabilities {
  generated_at_unix: number;
  kernel: string | null;
  hostname: string | null;
  dmi: {
    sys_vendor: string | null;
    product_name: string | null;
    product_version: string | null;
    board_name: string | null;
    bios_version: string | null;
  };
  ec: EcInfo;
  platform: PlatformInfo;
  power_supply: PowerSupplyInfo;
  cpu: CpuInfo;
  gpu: GpuInfo | null;
  hwmon: HwmonDevice[];
  ideapad: IdeapadInfo;
  leds: LedInfo[];
  tlp: TlpInfo;
  services: ServicesInfo;
  legion: LegionInfo;
}
