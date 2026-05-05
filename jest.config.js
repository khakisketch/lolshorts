/** @type {import('jest').Config} */
export default {
  preset: 'ts-jest',
  testEnvironment: 'jsdom',

  // Test file patterns
  testMatch: [
    '<rootDir>/src/**/*.test.{ts,tsx}', // Only run unit tests in src/
  ],

  // Ignore Playwright E2E tests
  testPathIgnorePatterns: [
    '/node_modules/',
    '/tests/e2e/', // Playwright E2E tests should run with `playwright test`
    '\\.spec\\.ts$', // Ignore all .spec.ts files (Playwright convention)
  ],

  // Global definitions for import.meta
  globals: {
    'import.meta': {
      env: {
        DEV: true,
        PROD: false,
        VITE_SUPABASE_URL: 'http://localhost:54321',
        VITE_SUPABASE_ANON_KEY: 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyAgCiAgICAicm9sZSI6ICJhbm9uIiwKICAgICJpc3MiOiAic3VwYWJhc2UtZGVtbyIsCiAgICAiaWF0IjogMTY0MTc2OTIwMCwKICAgICJleHAiOiAxNzk5NTM1NjAwCn0.dc_X5iR_VP_qT0zsiyj_I_OZ2T9FtRU2BBNWN8Bu4GE',
      },
    },
  },

  // TypeScript and module transformation
  transform: {
    '^.+\\.tsx?$': ['ts-jest', {
      tsconfig: {
        jsx: 'react-jsx',
        esModuleInterop: true,
        allowSyntheticDefaultImports: true,
      },
    }],
  },

  // Module name mappers for CSS and assets
  moduleNameMapper: {
    '^@/lib/logger$': '<rootDir>/__mocks__/loggerMock.ts', // Mock logger (import.meta)
    '^@/(.*)$': '<rootDir>/src/$1', // Path alias support
    '\\.(css|less|scss|sass)$': 'identity-obj-proxy', // Mock CSS imports
    '\\.(jpg|jpeg|png|gif|svg|woff|woff2|ttf|eot)$': '<rootDir>/__mocks__/fileMock.js', // Mock assets
    // Mock FFmpeg and binary dependencies
    'ffmpeg-static': '<rootDir>/__mocks__/ffmpegMock.js',
    'fluent-ffmpeg': '<rootDir>/__mocks__/ffmpegMock.js',
  },

  // Resolve modules with extensions
  moduleDirectories: ['node_modules', 'src'],

  // Setup files
  setupFilesAfterEnv: ['<rootDir>/jest.setup.js'],

  // Coverage configuration
  collectCoverageFrom: [
    'src/**/*.{ts,tsx}',
    '!src/**/*.d.ts',
    '!src/**/*.stories.{ts,tsx}',
    '!src/**/*.test.{ts,tsx}',
    '!src/main.tsx', // Entry point
  ],

  coverageThreshold: {
    global: {
      lines: 50,
      branches: 40,
      functions: 50,
    },
  },

  // Module file extensions
  moduleFileExtensions: ['ts', 'tsx', 'js', 'jsx', 'json'],

  // Ignore transforms for node_modules
  transformIgnorePatterns: [
    'node_modules/(?!(supabase|@supabase|@radix-ui|@dnd-kit)/)',
  ],
};
