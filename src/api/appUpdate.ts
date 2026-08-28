import { cmd, listenToEvent } from "./client";
import type { AppUpdateSnapshot } from "@/types/appUpdate";

export const APP_UPDATE_PROGRESS_EVENT = "app-update-progress";

export const appUpdateApi = {
  getStatus: () => cmd<AppUpdateSnapshot>("get_app_update_status"),
  check: () => cmd<AppUpdateSnapshot>("check_app_update"),
  install: () => cmd<AppUpdateSnapshot>("install_app_update"),
  listen: (callback: (snapshot: AppUpdateSnapshot) => void) =>
    listenToEvent<AppUpdateSnapshot>(APP_UPDATE_PROGRESS_EVENT, callback),
};
