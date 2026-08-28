import { create } from "zustand";
import { appUpdateApi } from "@/api/appUpdate";
import { AppError } from "@/api/client";
import type { AppUpdateSnapshot } from "@/types/appUpdate";

const INITIAL_SNAPSHOT: AppUpdateSnapshot = {
  status: "idle",
  current_version: "",
  available_version: null,
  notes: null,
  published_at: null,
  progress_percentage: 0,
  error_code: null,
};

interface AppUpdateStore {
  snapshot: AppUpdateSnapshot;
  initialized: boolean;
  initialize: () => Promise<() => void>;
  check: () => Promise<void>;
  install: () => Promise<void>;
}

const errorSnapshot = (
  current: AppUpdateSnapshot,
  error: unknown,
  fallbackCode: string,
): AppUpdateSnapshot => ({
  ...current,
  status: "failed",
  error_code: error instanceof AppError ? error.code : fallbackCode,
});

export const useAppUpdateStore = create<AppUpdateStore>((set, get) => ({
  snapshot: INITIAL_SNAPSHOT,
  initialized: false,

  initialize: async () => {
    if (get().initialized) return () => {};
    set({ initialized: true });
    const unlisten = await appUpdateApi.listen((snapshot) => set({ snapshot }));
    try {
      set({ snapshot: await appUpdateApi.getStatus() });
    } catch (error) {
      set(({ snapshot }) => ({
        snapshot: errorSnapshot(snapshot, error, "update_check_failed"),
      }));
    }
    return unlisten;
  },

  check: async () => {
    try {
      set({ snapshot: await appUpdateApi.check() });
    } catch (error) {
      set(({ snapshot }) => ({
        snapshot: errorSnapshot(snapshot, error, "update_check_failed"),
      }));
    }
  },

  install: async () => {
    try {
      set({ snapshot: await appUpdateApi.install() });
    } catch (error) {
      set(({ snapshot }) => ({
        snapshot: errorSnapshot(snapshot, error, "update_install_failed"),
      }));
    }
  },
}));
