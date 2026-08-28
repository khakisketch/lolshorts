export type AppUpdateStatus =
  | "disabled"
  | "idle"
  | "checking"
  | "up_to_date"
  | "available"
  | "downloading"
  | "installing"
  | "failed";

export interface AppUpdateSnapshot {
  status: AppUpdateStatus;
  current_version: string;
  available_version: string | null;
  notes: string | null;
  published_at: string | null;
  progress_percentage: number;
  error_code: string | null;
}
