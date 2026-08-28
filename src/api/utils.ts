import { cmd } from "./client";
import type { StorageStats } from "@/types/storage";

export interface DiskSpaceInfo {
  known: boolean;
  available_gb: number;
  total_gb: number;
  used_gb: number;
}

export interface SystemMetrics {
  total_cpu_percent: number;
  available_ram_gb: number;
  available_disk_gb: number;
  gpu_percent: number | null;
  gpu_memory_mb: number | null;
  gpu_temperature_celsius: number | null;
}

export type DiagnosticState = "ok" | "warning" | "blocked";

export interface DiagnosticCheck {
  key: string;
  label: string;
  status: DiagnosticState;
  message: string;
  action: string;
}

export interface DiagnosticsStatus {
  overall_status: DiagnosticState;
  checks: DiagnosticCheck[];
}

export interface DiagnosticsBundleExport {
  output_path: string;
  redacted: boolean;
  generated_at: string;
  included_logs: number;
}

export interface StagedMedia {
  path: string;
  size_bytes: number;
  reused_app_owned_file: boolean;
  original_file_name: string;
}

export const utilsApi = {
  // File System
  showInFolder: (filePath: string) =>
    cmd<void>("show_in_folder", { file_path: filePath }),

  openFileWithDefaultApp: (filePath: string) =>
    cmd<void>("open_file_with_default_app", { file_path: filePath }),

  checkFileExists: (filePath: string) =>
    cmd<boolean>("check_file_exists", { file_path: filePath }),

  // System
  getAppVersion: () => cmd<string>("get_app_version"),
  getDiskSpaceInfo: () => cmd<DiskSpaceInfo>("get_disk_space_info"),
  getSystemMetrics: () => cmd<SystemMetrics>("get_system_metrics"),
  cleanupTempFiles: () => cmd<void>("cleanup_temp_files"),
  forceCleanup: () => cmd<void>("force_cleanup"),

  // Dashboard Stats
  getDashboardStats: () => cmd<StorageStats>("get_dashboard_stats"),

  // Diagnostics
  getDiagnosticsStatus: () => cmd<DiagnosticsStatus>("get_diagnostics_status"),
  exportDiagnosticsBundle: (redact = true) =>
    cmd<DiagnosticsBundleExport>("export_diagnostics_bundle", { redact }),

  selectAndStageExternalMedia: (kind: "video" | "audio" | "image") =>
    cmd<StagedMedia | null>("select_and_stage_external_media", { kind }),
};
