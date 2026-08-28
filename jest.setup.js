// Jest setup file for testing environment
require('@testing-library/jest-dom');

// Keep the suite honest: React act warnings and accidental application logging
// are test failures. The logger contract suite deliberately exercises console
// forwarding behind its own spies, so it is the sole scoped exception.
let unexpectedConsoleError;
let unexpectedConsoleWarn;
beforeEach(() => {
  unexpectedConsoleError = jest.spyOn(console, 'error').mockImplementation(() => {});
  unexpectedConsoleWarn = jest.spyOn(console, 'warn').mockImplementation(() => {});
});
afterEach(() => {
  const testPath = expect.getState().testPath || '';
  const isLoggerContract = testPath.endsWith('src\\lib\\__tests__\\logger.test.ts') ||
    testPath.endsWith('src/lib/__tests__/logger.test.ts');
  const errorCalls = unexpectedConsoleError.mock.calls;
  const warnCalls = unexpectedConsoleWarn.mock.calls;
  jest.restoreAllMocks();
  if (!isLoggerContract && (errorCalls.length > 0 || warnCalls.length > 0)) {
    const rendered = [...errorCalls, ...warnCalls]
      .map((args) => args.map((value) => String(value)).join(' '))
      .join('\n');
    throw new Error(`Unexpected console error/warn:\n${rendered}`);
  }
});

// Mock ResizeObserver (required by Radix UI components)
global.ResizeObserver = jest.fn().mockImplementation(() => ({
  observe: jest.fn(),
  unobserve: jest.fn(),
  disconnect: jest.fn(),
}));

// Mock Tauri API since it's not available in test environment
global.window = global.window || {};
global.window.__TAURI__ = {
  invoke: jest.fn(),
  event: {
    listen: jest.fn(),
    emit: jest.fn(),
  },
  dialog: {
    ask: jest.fn(),
    confirm: jest.fn(),
    message: jest.fn(),
    open: jest.fn(),
    save: jest.fn(),
  },
  shell: {
    open: jest.fn(),
    command: jest.fn(),
  },
};

// Enhanced Tauri invoke mock with specific functions
const mockInvoke = jest.fn();
mockInvoke.mockImplementation(async (cmd, args) => {
  // Mock common Tauri commands
  switch (cmd) {
    case 'get_auth_status':
      return { authenticated: false, tier: 'FREE' };
    case 'list_audio_devices':
      return [
        { id: 'default', name: 'Default Audio Device', device_type: 'SystemAudio' },
        { id: 'microphone', name: 'Microphone', device_type: 'Microphone' }
      ];
    case 'get_recording_status':
      return 'idle';
    case 'get_detailed_recording_status':
      return {
        status: 'Idle',
        is_monitoring: false,
        buffer_duration_secs: 120
      };
    case 'get_saved_clips':
      return [];
    case 'get_system_info':
      return {
        platform: 'win32',
        arch: 'x64',
        version: '1.2.0'
      };
    case 'get_app_update_status':
      return {
        status: 'disabled',
        current_version: '1.2.0',
        available_version: null,
        notes: null,
        published_at: null,
        progress_percentage: 0,
        error_code: 'updater_disabled',
      };
    default:
      return null;
  }
});

global.window.__TAURI__.invoke = mockInvoke;

// Mock matchMedia (required by some UI components)
Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: jest.fn().mockImplementation((query) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: jest.fn(), // deprecated
    removeListener: jest.fn(), // deprecated
    addEventListener: jest.fn(),
    removeEventListener: jest.fn(),
    dispatchEvent: jest.fn(),
  })),
});

// Mock import.meta.env for ESM modules
Object.defineProperty(global, 'import', {
  value: {
    meta: {
      env: {
        VITE_SUPABASE_URL: 'http://localhost:54321',
        VITE_SUPABASE_ANON_KEY: 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyAgCiAgICAicm9sZSI6ICJhbm9uIiwKICAgICJpc3MiOiAic3VwYWJhc2UtZGVtbyIsCiAgICAiaWF0IjogMTY0MTc2OTIwMCwKICAgICJleHAiOiAxNzk5NTM1NjAwCn0.dc_X5iR_VP_qT0zsiyj_I_OZ2T9FtRU2BBNWN8Bu4GE',
      },
    },
  },
  writable: true,
});

// Define process.env for non-module contexts
process.env = {
  NODE_ENV: 'test',
  VITE_SUPABASE_URL: 'http://localhost:54321',
  VITE_SUPABASE_ANON_KEY: 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyAgCiAgICAicm9sZSI6ICJhbm9uIiwKICAgICJpc3MiOiAic3VwYWJhc2UtZGVtbyIsCiAgICAiaWF0IjogMTY0MTc2OTIwMCwKICAgICJleHAiOiAxNzk5NTM1NjAwCn0.dc_X5iR_VP_qT0zsiyj_I_OZ2T9FtRU2BBNWN8Bu4GE',
};

// Mock react-i18next to prevent initialization warnings
jest.mock('react-i18next', () => ({
  useTranslation: () => ({
    // Mirror i18next's `defaultValue` handling. Returning the key unconditionally
    // made every `t(key, { defaultValue })` call site render the key in tests while
    // the real app renders the fallback — a green test proving the opposite of what
    // the user sees.
    t: (key, opts) =>
      opts && Object.prototype.hasOwnProperty.call(opts, 'defaultValue')
        ? opts.defaultValue
        : key,
    i18n: {
      language: 'en',
      changeLanguage: jest.fn(),
    },
  }),
  initReactI18next: {},
}));

// Mock i18next
jest.mock('i18next', () => ({
  init: jest.fn(() => Promise.resolve()),
  t: (key) => key,
}));

// Mock sessionStorage
Object.defineProperty(window, 'sessionStorage', {
  value: {
    getItem: jest.fn(),
    setItem: jest.fn(),
    removeItem: jest.fn(),
    clear: jest.fn(),
  },
  writable: true,
});

// Mock client.ts to avoid import.meta.env issues
jest.mock('./src/api/client', () => ({
  cmd: jest.fn().mockResolvedValue(null),
  AppError: class AppError extends Error {
    constructor(response) {
      super(response.message);
      this.code = response.code;
      this.name = 'AppError';
    }
  },
  validateString: jest.fn((v) => v),
  validatePath: jest.fn((v) => v),
  validateNumber: jest.fn((v) => v),
  validateEmail: jest.fn((v) => v),
}));

// Mock auth module to avoid import.meta.env issues
jest.mock('./src/lib/auth', () => ({
  useAuthStore: jest.fn(() => ({
    user: null,
    isAuthenticated: false,
    isLoading: false,
    error: null,
    login: jest.fn(),
    loginWithGoogle: jest.fn(),
    signup: jest.fn(),
    logout: jest.fn(),
    refreshToken: jest.fn(),
    checkAuth: jest.fn(),
    getLicenseInfo: jest.fn(),
    clearError: jest.fn(),
    startTokenRefresh: jest.fn(),
    stopTokenRefresh: jest.fn(),
  })),
  // Re-export types for test compatibility
  UserProfile: {},
  User: {},
  LoginCredentials: {},
  SignupCredentials: {},
  LicenseInfo: {},
}));

// Mock errorMapper to avoid import issues
jest.mock('./src/lib/errorMapper', () => ({
  getErrorKey: jest.fn((error) => 'errors.generic'),
  getErrorKeyFromCode: jest.fn(() => 'errors.generic'),
  isNetworkError: jest.fn(() => false),
  isAuthError: jest.fn(() => false),
}));

// Mock supabase to avoid import.meta.env issues
jest.mock('./src/lib/supabase', () => ({
  supabase: {
    from: jest.fn(() => ({
      select: jest.fn(() => ({
        eq: jest.fn(() => ({
          single: jest.fn(() => Promise.resolve({ data: null, error: null }))
        })),
        order: jest.fn(() => Promise.resolve({ data: [], error: null }))
      })),
      insert: jest.fn(() => Promise.resolve({ data: null, error: null })),
      update: jest.fn(() => ({
        eq: jest.fn(() => Promise.resolve({ data: null, error: null }))
      })),
      delete: jest.fn(() => ({
        eq: jest.fn(() => Promise.resolve({ data: null, error: null }))
      }))
    })),
    auth: {
      getUser: jest.fn(() => Promise.resolve({ data: { user: null }, error: null })),
      signInWithOAuth: jest.fn(),
      signInWithPassword: jest.fn(),
      signOut: jest.fn(() => Promise.resolve({ error: null })),
      onAuthStateChange: jest.fn(() => ({ data: { subscription: { unsubscribe: jest.fn() } } }))
    }
  }
}));
