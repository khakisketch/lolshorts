/**
 * 에러 메시지를 i18n 키로 매핑하는 유틸리티
 *
 * Supabase 및 기타 서비스의 기술적 에러 메시지를
 * 사용자 친화적인 번역 키로 변환합니다.
 */

// 백엔드 에러 코드 → i18n 키 매핑
const BACKEND_ERROR_MAP: Record<string, string> = {
  DISK_FULL: "errors.diskFull",
  FFMPEG_NOT_FOUND: "errors.ffmpegNotFound",
  NETWORK_ERROR: "errors.networkError",
  AUTH_EXPIRED: "errors.authExpired",
  PROCESS_TIMEOUT: "errors.processTimeout",
  RATE_LIMITED: "errors.rateLimited",
  OUT_OF_MEMORY: "errors.outOfMemory",
  CORRUPTED_FILE: "errors.corruptedFile",
  DEVICE_DISCONNECTED: "errors.deviceDisconnected",
  SERVICE_UNAVAILABLE: "errors.serviceUnavailable",
};

/**
 * 백엔드 에러 코드를 i18n 키로 변환
 */
export function mapBackendError(code: string): string {
  return BACKEND_ERROR_MAP[code] || "errors.unknown";
}

// Supabase 에러 코드 → i18n 키 매핑
const AUTH_ERROR_MAP: Record<string, string> = {
  // Supabase Auth 에러 코드
  invalid_credentials: "errors.invalidCredentials",
  invalid_grant: "errors.invalidCredentials",
  user_not_found: "errors.invalidCredentials",
  invalid_password: "errors.invalidCredentials",
  email_not_confirmed: "errors.invalidCredentials",
  user_already_registered: "errors.emailAlreadyInUse",
  email_already_in_use: "errors.emailAlreadyInUse",
  weak_password: "errors.weakPassword",
  invalid_email: "errors.invalidEmail",
  user_banned: "errors.accountDisabled",
  user_disabled: "errors.accountDisabled",
  over_request_rate_limit: "errors.tooManyRequests",
  rate_limit_exceeded: "errors.tooManyRequests",
  session_expired: "errors.sessionExpired",
  refresh_token_not_found: "errors.sessionExpired",
};

// 에러 메시지 문자열 → i18n 키 매핑 (Supabase에서 코드 없이 메시지만 오는 경우)
const ERROR_MESSAGE_MAP: Record<string, string> = {
  "Invalid login credentials": "errors.invalidCredentials",
  "User already registered": "errors.emailAlreadyInUse",
  "Password should be at least 6 characters": "errors.weakPassword",
  "Unable to validate email address: invalid format": "errors.invalidEmail",
  "Email rate limit exceeded": "errors.tooManyRequests",
  "For security purposes, you can only request this once every 60 seconds":
    "errors.tooManyRequests",
};

// 일반적인 에러 패턴 매핑
const ERROR_PATTERN_MAP: Array<{ pattern: RegExp; key: string }> = [
  { pattern: /password/i, key: "errors.invalidCredentials" },
  { pattern: /email.*invalid/i, key: "errors.invalidEmail" },
  { pattern: /already.*registered/i, key: "errors.emailAlreadyInUse" },
  { pattern: /rate.*limit/i, key: "errors.tooManyRequests" },
  { pattern: /session.*expired/i, key: "errors.sessionExpired" },
  { pattern: /network/i, key: "errors.networkError" },
  { pattern: /fetch/i, key: "errors.networkError" },
];

interface SupabaseError {
  code?: string;
  message?: string;
  status?: number;
}

/**
 * 에러를 i18n 키로 변환
 * @param error - 에러 객체 또는 문자열
 * @returns i18n 번역 키
 */
export function getErrorKey(error: unknown): string {
  // null/undefined 처리
  if (!error) {
    return "errors.generic";
  }

  // 문자열 에러
  if (typeof error === "string") {
    return mapErrorMessage(error);
  }

  // Error 객체
  if (error instanceof Error) {
    // Supabase AuthError 스타일
    const supabaseError = error as Error & SupabaseError;

    // 에러 코드로 매핑 시도
    if (supabaseError.code && BACKEND_ERROR_MAP[supabaseError.code]) {
      return BACKEND_ERROR_MAP[supabaseError.code];
    }
    if (supabaseError.code && AUTH_ERROR_MAP[supabaseError.code]) {
      return AUTH_ERROR_MAP[supabaseError.code];
    }

    // 메시지로 매핑 시도
    return mapErrorMessage(supabaseError.message);
  }

  // 일반 객체
  if (typeof error === "object") {
    const obj = error as SupabaseError;

    // 에러 코드로 매핑 시도
    if (obj.code && BACKEND_ERROR_MAP[obj.code]) {
      return BACKEND_ERROR_MAP[obj.code];
    }
    if (obj.code && AUTH_ERROR_MAP[obj.code]) {
      return AUTH_ERROR_MAP[obj.code];
    }

    // 메시지로 매핑 시도
    if (obj.message) {
      return mapErrorMessage(obj.message);
    }
  }

  return "errors.generic";
}

/**
 * 에러 메시지 문자열을 i18n 키로 변환
 */
function mapErrorMessage(message: string): string {
  // 정확한 매칭 시도
  if (ERROR_MESSAGE_MAP[message]) {
    return ERROR_MESSAGE_MAP[message];
  }

  // 패턴 매칭 시도
  for (const { pattern, key } of ERROR_PATTERN_MAP) {
    if (pattern.test(message)) {
      return key;
    }
  }

  return "errors.generic";
}

/**
 * 에러 코드를 i18n 키로 직접 변환 (알려진 에러 코드용)
 */
export function getErrorKeyFromCode(code: string): string {
  return BACKEND_ERROR_MAP[code] || AUTH_ERROR_MAP[code] || "errors.generic";
}

/**
 * 특정 에러 타입인지 확인
 */
export function isNetworkError(error: unknown): boolean {
  if (error instanceof Error) {
    if ((error as Error & { code?: string }).code === "NETWORK_ERROR") {
      return true;
    }
    return /network|fetch|ECONNREFUSED|ETIMEDOUT/i.test(error.message);
  }
  return false;
}

export function isAuthError(error: unknown): boolean {
  if (error instanceof Error) {
    const supabaseError = error as Error & SupabaseError;
    return (
      supabaseError.code !== undefined &&
      Object.keys(AUTH_ERROR_MAP).includes(supabaseError.code)
    );
  }
  return false;
}
