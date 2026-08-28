import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

let permissionChecked = false;
let hasPermission = false;

/**
 * 알림 권한 확인 및 요청
 */
async function ensurePermission(): Promise<boolean> {
  if (permissionChecked) return hasPermission;

  try {
    hasPermission = await isPermissionGranted();
    if (!hasPermission) {
      const permission = await requestPermission();
      hasPermission = permission === "granted";
    }
    permissionChecked = true;
  } catch {
    hasPermission = false;
    permissionChecked = true;
  }
  return hasPermission;
}

/**
 * 데스크톱 알림 전송
 */
export async function notify(title: string, body: string): Promise<void> {
  const permitted = await ensurePermission();
  if (!permitted) return;

  try {
    sendNotification({ title, body });
  } catch {
    // 알림 실패는 무시
  }
}

/**
 * 게임 시작 알림
 */
export async function notifyGameStarted(champion?: string): Promise<void> {
  const body = champion
    ? `${champion} 게임 시작 - 녹화가 활성화되었습니다`
    : "게임 시작 - 녹화가 활성화되었습니다";
  await notify("LoLShorts", body);
}

/**
 * 게임 종료 알림
 */
export async function notifyGameEnded(clipCount: number): Promise<void> {
  await notify("LoLShorts", `게임 종료 - ${clipCount}개 클립 저장됨`);
}

/**
 * 클립 저장 알림
 */
export async function notifyClipSaved(clipName: string): Promise<void> {
  await notify("LoLShorts", `클립 저장됨: ${clipName}`);
}

/**
 * 업로드 완료 알림
 */
export async function notifyUploadComplete(videoTitle: string): Promise<void> {
  await notify("LoLShorts", `업로드 완료: ${videoTitle}`);
}
